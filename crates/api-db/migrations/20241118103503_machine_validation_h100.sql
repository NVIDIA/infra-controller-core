UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_DcgmFullLong'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_DcgmFullShort'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_MqStresserLong'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_MqStresserShort'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_CPUTestLong'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_CPUTestShort'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_MemoryTestLong'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_MemoryTestShort'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '920-24387-2540-000')
WHERE
    test_id = 'nico_NicoRunBook'
    AND array_position(supported_platforms, '920-24387-2540-000') IS NULL;

-- Testing is completed; Remove default sku from all test cases
UPDATE
    machine_validation_tests
SET
    supported_platforms = array_remove(supported_platforms, 'default');

INSERT INTO
    machine_validation_tests (
        args,
        command,
        components,
        contexts,
        description,
        name,
        read_only,
        supported_platforms,
        timeout,
        version,
        test_id,
        pre_condition,
        custom_tags,
        modified_by
    )
VALUES
    (
        'This is default Test Case',
        'echo ',
        ARRAY ['Compute'],
        ARRAY ['Discovery','Cleanup','OnDemand'],
        'This is default test case',
        'DefaultTestCase',
        true,
        ARRAY ['default'],
        7200,
        'V1-T1731384539962561',
        'nico_DefaultTestCase',
        '',
        ARRAY ['dgxcloud'],
        'System'
    );