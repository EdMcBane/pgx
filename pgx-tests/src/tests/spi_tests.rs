/*
Portions Copyright 2019-2021 ZomboDB, LLC.
Portions Copyright 2021-2022 Technology Concepts & Design, Inc. <support@tcdi.com>

All rights reserved.

Use of this source code is governed by the MIT license that can be found in the LICENSE file.
*/

#[cfg(any(test, feature = "pg_test"))]
#[pgx::pg_schema]
mod tests {
    #[allow(unused_imports)]
    use crate as pgx_tests;

    use pgx::*;

    #[pg_test(error = "syntax error at or near \"THIS\"")]
    fn test_spi_failure() {
        Spi::execute(|client| {
            client.select("THIS IS NOT A VALID QUERY", None, None);
        });
    }

    #[pg_test]
    fn test_spi_can_nest() {
        Spi::execute(|_| {
            Spi::execute(|_| {
                Spi::execute(|_| {
                    Spi::execute(|_| {
                        Spi::execute(|_| {});
                    });
                });
            });
        });
    }

    #[pg_test]
    fn test_spi_returns_primitive() {
        let rc = Spi::connect(|client| {
            Ok(client
                .select("SELECT 42", None, None)
                .first()
                .get_datum::<i32>(1))
        });

        assert_eq!(42, rc.expect("SPI failed to return proper value"))
    }

    #[pg_test]
    fn test_spi_returns_str() {
        let rc = Spi::connect(|client| {
            Ok(client
                .select("SELECT 'this is a test'", None, None)
                .first()
                .get_datum::<&str>(1))
        });

        assert_eq!(
            "this is a test",
            rc.expect("SPI failed to return proper value")
        )
    }

    #[pg_test]
    fn test_spi_returns_string() {
        let rc = Spi::connect(|client| {
            Ok(client
                .select("SELECT 'this is a test'", None, None)
                .first()
                .get_datum::<String>(1))
        });

        assert_eq!(
            "this is a test",
            rc.expect("SPI failed to return proper value")
        )
    }

    #[pg_test]
    fn test_spi_get_one() {
        Spi::execute(|client| {
            let i = client
                .select("SELECT 42::bigint", None, None)
                .first()
                .get_one::<i64>();
            assert_eq!(42, i.unwrap());
        });
    }

    #[pg_test]
    fn test_spi_get_two() {
        Spi::execute(|client| {
            let (i, s) = client
                .select("SELECT 42, 'test'", None, None)
                .first()
                .get_two::<i64, &str>();

            assert_eq!(42, i.unwrap());
            assert_eq!("test", s.unwrap());
        });
    }

    #[pg_test]
    fn test_spi_get_three() {
        Spi::execute(|client| {
            let (i, s, b) = client
                .select("SELECT 42, 'test', true", None, None)
                .first()
                .get_three::<i64, &str, bool>();

            assert_eq!(42, i.unwrap());
            assert_eq!("test", s.unwrap());
            assert_eq!(true, b.unwrap());
        });
    }

    #[pg_test]
    fn test_spi_get_two_with_failure() {
        Spi::execute(|client| {
            let (i, s) = client
                .select("SELECT 42", None, None)
                .first()
                .get_two::<i64, &str>();

            assert_eq!(42, i.unwrap());
            assert!(s.is_none());
        });
    }

    #[pg_test]
    fn test_spi_get_three_failure() {
        Spi::execute(|client| {
            let (i, s, b) = client
                .select("SELECT 42, 'test'", None, None)
                .first()
                .get_three::<i64, &str, bool>();

            assert_eq!(42, i.unwrap());
            assert_eq!("test", s.unwrap());
            assert!(b.is_none());
        });
    }

    #[pg_test]
    fn test_spi_select_zero_rows() {
        assert!(Spi::get_one::<i32>("SELECT 1 LIMIT 0").is_none());
    }

    #[pg_extern]
    fn do_panic() {
        panic!("did a panic");
    }

    #[pg_test(error = "did a panic")]
    fn test_panic_via_spi() {
        Spi::run("SELECT tests.do_panic();");
    }

    #[pg_test]
    fn test_inserting_null() {
        Spi::execute(|mut client| {
            client.update("CREATE TABLE tests.null_test (id uuid)", None, None);
        });
        let result = Spi::get_one_with_args::<i32>(
            "INSERT INTO tests.null_test VALUES ($1) RETURNING 1",
            vec![(PgBuiltInOids::UUIDOID.oid(), None)],
        );
        assert_eq!(result, Some(1));
    }

    #[pg_test]
    fn test_cursor() {
        Spi::execute(|mut client| {
            client.update("CREATE TABLE tests.cursor_table (id int)", None, None);
            client.update("INSERT INTO tests.cursor_table (id) \
            SELECT i FROM generate_series(1, 10) AS t(i)", None, None);
            let mut portal = client.open_cursor("SELECT * FROM tests.cursor_table", None);

            fn sum_all(table: SpiTupleTable) -> i32 {
                table.map(|r| r.by_ordinal(1).unwrap().value::<i32>().unwrap()).sum()
            }
            assert_eq!(sum_all(portal.fetch(3)), 1+2+3);
            assert_eq!(sum_all(portal.fetch(3)), 4+5+6);
            assert_eq!(sum_all(portal.fetch(3)), 7+8+9);
            assert_eq!(sum_all(portal.fetch(3)), 10);
        });
    }

    #[pg_test]
    fn test_cursor_by_name() {
        let cursor_name = Spi::connect(|mut client| {
            client.update("CREATE TABLE tests.cursor_table (id int)", None, None);
            client.update("INSERT INTO tests.cursor_table (id) \
            SELECT i FROM generate_series(1, 10) AS t(i)", None, None);
            let mut cursor = client.open_cursor("SELECT * FROM tests.cursor_table", None);
            assert_eq!(sum_all(cursor.fetch(3)), 1+2+3);
            Ok(Some(cursor.into_name()))
        }).unwrap();

        fn sum_all(table: SpiTupleTable) -> i32 {
            table.map(|r| r.by_ordinal(1).unwrap().value::<i32>().unwrap()).sum()
        }
        Spi::connect(|mut client| {
            let mut cursor = client.find_cursor(cursor_name.clone());
            assert_eq!(sum_all(cursor.fetch(3)), 4+5+6);
            assert_eq!(sum_all(cursor.fetch(3)), 7+8+9);
            Ok(None::<()>)
        });

        Spi::connect(|mut client| {
            let mut cursor = client.find_cursor(cursor_name);
            assert_eq!(sum_all(cursor.fetch(3)), 10);
            Ok(None::<()>)
        });
    }

    #[pg_test(error = "syntax error at or near \"THIS\"")]
    fn test_cursor_failure() {
        Spi::execute(|mut client| {
            client.open_cursor("THIS IS NOT SQL", None);
        });
    }

    #[pg_test]
    fn test_cursor_close() {
        Spi::connect(|mut client| {
            let cursor = client.open_cursor("SELECT 1", None);
            cursor.close();
            Ok(None::<()>)
        });
    }

    #[pg_test(error="cursor named \"NOT A CURSOR\" not found")]
    fn test_cursor_not_found() {
        Spi::connect(|mut client| {
            client.find_cursor("NOT A CURSOR".to_string());
            Ok(None::<()>)
        });
    }
}
