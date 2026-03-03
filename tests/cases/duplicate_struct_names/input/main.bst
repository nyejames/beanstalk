-- Test duplicate struct names should be detected

Person |name String, age Int|

Person |first_name String, last_name String|

person = Person { name: "Alice", age: 30 }
["Created person"]
