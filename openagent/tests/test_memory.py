import asyncio
import lancedb

async def list_memory_indexes():
    # Connect to the local LanceDB directory based on db.rs configuration
    db = await lancedb.connect_async("data/memory")

    # list_tables() returns a paged result object — extract .tables for the list
    result = await db.list_tables()
    table_names = result.tables
    print("Memory Indexes:", table_names)

    for table_name in table_names:
        print(f"\n--- Documents in '{table_name}' ---")
        try:
            table = await db.open_table(table_name)

            # schema() is a method in this lancedb version
            schema = await table.schema()
            print("  Schema:")
            for field in schema:
                print(f"    - {field.name}: {field.type}")

            # Query all rows, including the vector column
            results = (
                await table.query()
                .select(["id", "content", "metadata", "created_at", "vector"])
                .limit(10)
                .to_list()
            )

            if not results:
                print("  (Empty)")
            else:
                for i, row in enumerate(results):
                    vector = row.get("vector") or []
                    # Show first 5 floats as a preview to confirm data is present
                    preview = f"[{', '.join(f'{v:.4f}' for v in vector[:5])}...] ({len(vector)} dims)"
                    print(f"  [{i+1}] ID:      {row.get('id')}")
                    print(f"       Content:  {row.get('content')}")
                    print(f"       Created:  {row.get('created_at')}")
                    print(f"       Metadata: {row.get('metadata')}")
                    print(f"       Vector:   {preview}")

        except Exception as e:
            print(f"  (Error: {e})")

asyncio.run(list_memory_indexes())
