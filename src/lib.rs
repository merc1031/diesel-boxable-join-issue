pub mod schema {
    pub mod subjects {
        use diesel::*;

        table! {
            subjects(id) {
                id -> Integer,
                name -> Text,
            }
        }
    }

    pub mod relations {
        use diesel::*;

        table! {
            relations(id) {
                id -> Integer,
                key -> VarChar,
                value -> VarChar,
            }
        }
    }

    pub mod many_to_manys {
        use diesel::*;

        table! {
            many_to_manys(id) {
                id -> Integer,
                subject_id -> Integer,
                relation_id -> Integer,
            }
        }
    }

    use many_to_manys::many_to_manys as m;
    use relations::relations as r;
    use subjects::subjects as s;

    diesel::joinable!(many_to_manys::many_to_manys -> subjects::subjects(subject_id));
    diesel::joinable!(many_to_manys::many_to_manys -> relations::relations(relation_id));

    diesel::allow_tables_to_appear_in_same_query!(s, m, r);
}

pub mod models {
    pub mod subject {
        use diesel::{Identifiable, Queryable, Selectable};

        use super::super::schema::subjects::subjects;

        #[derive(Identifiable, Queryable, Selectable)]
        #[diesel(table_name = subjects)]
        pub struct Subject {
            pub id: i32,
            pub name: String,
        }
    }

    pub mod relation {
        use diesel::{Identifiable, Queryable, Selectable};

        use super::super::schema::relations::relations;

        #[derive(Identifiable, Queryable, Selectable)]
        #[diesel(table_name = relations)]
        pub struct Relation {
            pub id: i32,
            pub key: String,
            pub value: String,
        }
    }

    pub mod many_to_many {
        use diesel::{prelude::Associations, Identifiable, Queryable, Selectable};

        use super::{
            super::schema::many_to_manys::many_to_manys,
            relation::Relation,
            subject::Subject,
        };

        #[derive(Associations, Identifiable, Queryable, Selectable)]
        #[diesel(belongs_to(Subject))]
        #[diesel(belongs_to(Relation))]
        #[diesel(table_name = many_to_manys)]
        pub struct ManyToMany {
            pub id: i32,
            pub subject_id: i32,
            pub relation_id: i32,
        }
    }
}

mod example {
    use anyhow::Result;
    use diesel::{
        dsl::count_distinct,
        expression::is_aggregate,
        pg::Pg,
        query_builder::BoxedSelectStatement,
        sql_types::Bool,
        BoolExpressionMethods,
        BoxableExpression,
        ExpressionMethods,
        JoinOnDsl,
        QueryDsl,
        SelectableHelper,
    };
    use diesel_async::{
        pooled_connection::{bb8::Pool, AsyncDieselConnectionManager},
        AsyncPgConnection,
        RunQueryDsl,
    };

    use super::schema::{
        many_to_manys::many_to_manys::dsl as many_to_manys_dsl,
        relations::relations::dsl as relations_dsl,
        subjects::subjects::dsl as subjects_dsl,
    };
    use crate::models::subject::Subject;

    pub struct Conditions {
        pub relation_one: Option<String>,
        pub relation_two: Option<String>,
        pub relation_three: Option<String>,
        pub relation_four: Option<String>,
    }

    impl Iterator for Conditions {
        type Item = (String, String);

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(relation_one) = self.relation_one.take() {
                return Some(("relation_one".to_string(), relation_one));
            }
            if let Some(relation_two) = self.relation_two.take() {
                return Some(("relation_two".to_string(), relation_two));
            }
            if let Some(relation_three) = self.relation_three.take() {
                return Some(("relation_three".to_string(), relation_three));
            }
            if let Some(relation_four) = self.relation_four.take() {
                return Some(("relation_four".to_string(), relation_four));
            }

            None
        }
    }

    async fn dynamic_filter_many_to_many_simple() -> Result<()> {
        let config =
            AsyncDieselConnectionManager::<AsyncPgConnection>::new("postgres://localhost:5432/db");
        let pool = Pool::builder().max_size(10).build(config).await?;

        let mut connection = pool.get().await?;

        let conditions = Conditions {
            relation_one: Some("foo".to_string()),
            relation_two: Some("bar".to_string()),
            relation_three: None,
            relation_four: None,
        };
        let (condition_count, dyn_where) = create_filter(conditions);

        let query = subjects_dsl::subjects
            .inner_join(
                many_to_manys_dsl::many_to_manys
                    .on(many_to_manys_dsl::subject_id.eq(subjects_dsl::id)),
            )
            .inner_join(
                relations_dsl::relations.on(many_to_manys_dsl::relation_id.eq(relations_dsl::id)),
            )
            .filter(subjects_dsl::name.eq_any(&["foo", "bar"]))
            .filter(dyn_where)
            .select(Subject::as_select())
            .order(subjects_dsl::id.desc())
            .limit(2);

        let result = query.load::<Subject>(&mut connection).await?;
        Ok(())
    }

    async fn dynamic_filter_many_to_many() -> Result<()> {
        let config =
            AsyncDieselConnectionManager::<AsyncPgConnection>::new("postgres://localhost:5432/db");
        let pool = Pool::builder().max_size(10).build(config).await?;

        let mut connection = pool.get().await?;

        let conditions = Conditions {
            relation_one: Some("foo".to_string()),
            relation_two: Some("bar".to_string()),
            relation_three: None,
            relation_four: None,
        };
        let (condition_count, dyn_where) = create_filter(conditions);

        let query = subjects_dsl::subjects
            .inner_join(
                many_to_manys_dsl::many_to_manys
                    .on(many_to_manys_dsl::subject_id.eq(subjects_dsl::id)),
            )
            .inner_join(
                relations_dsl::relations.on(many_to_manys_dsl::relation_id.eq(relations_dsl::id)),
            )
            .filter(subjects_dsl::name.eq_any(&["foo", "bar"]))
            .filter(dyn_where)
            .group_by(subjects_dsl::id)
            .having(count_distinct(relations_dsl::key).eq(condition_count))
            .select(Subject::as_select())
            .order(subjects_dsl::id.desc())
            .limit(2);

        let result = query.load::<Subject>(&mut connection).await?;
        Ok(())
    }

    type RelationJoin = diesel::dsl::InnerJoinQuerySource<
        super::schema::subjects::subjects::table,
        diesel::dsl::InnerJoinQuerySource<
            super::schema::many_to_manys::many_to_manys::table,
            super::schema::relations::relations::table,
        >,
    >;

    fn create_filter(
        conditions: Conditions,
    ) -> (
        i64,
        Box<dyn BoxableExpression<RelationJoin, Pg, (), is_aggregate::No, SqlType = Bool>>,
    ) {
        let mut condition_count = 0;
        let mut where_clause = Box::new(diesel::dsl::sql::<Bool>("TRUE"))
            as Box<dyn BoxableExpression<_, _, (), _, SqlType = Bool>>;
        for (key, value) in conditions.into_iter() {
            condition_count += 1;
            where_clause = Box::new(
                where_clause.or(relations_dsl::key
                    .eq(key.to_string())
                    .and(relations_dsl::value.eq(value))),
            ) as Box<dyn BoxableExpression<_, _, (), _, SqlType = Bool>>
        }
        (condition_count, where_clause)
    }
}
