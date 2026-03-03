-- Test file: Lifetime inference integration with drop insertion (should pass)
-- Tests that corrected lifetime spans enable accurate Drop node placement

-- Create data that needs cleanup
resource = "resource data"

-- Use the resource
processed = resource

-- Create another resource
another_resource = "another resource"
final_result = another_resource

-- Both resources should have drops inserted at correct points based on lifetime analysis