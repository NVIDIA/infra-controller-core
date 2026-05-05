UPDATE
    machine_validation_tests
SET
    pre_condition = '/opt/forge/dcgmi-pre.sh'
where
    test_id = 'nico_DcgmFullShort';

UPDATE
    machine_validation_tests
SET
    pre_condition = '/opt/forge/dcgmi-pre.sh'
where
    test_id = 'nico_DcgmFullLong';