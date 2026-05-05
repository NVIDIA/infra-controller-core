update
    machine_validation_tests
set
    external_config_file = '/tmp/machine_validation/external_config/shoreline'
where
    test_id = 'nico_NicoRunBook';

update
    machine_validation_tests
set
    verified = true;