UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '7d9ecto1ww')
WHERE
    test_id IN (
        'nico_DcgmFullLong',
        'nico_DcgmFullShort',
        'nico_MqStresserLong',
        'nico_MqStresserShort',
        'nico_CPUTestLong',
        'nico_CPUTestShort',
        'nico_MemoryTestLong',
        'nico_MemoryTestShort',
        'nico_NicoRunBook',
        'nico_CpuBenchmarkingFp',
        'nico_CpuBenchmarkingInt',
        'nico_CudaSample',
        'nico_FioPath',
        'nico_FioSSD',
        'nico_FioFile',
        'nico_MmMemBandwidth',
        'nico_MmMemLatency',
        'nico_MmMemPeakBandwidth',
        'nico_Nvbandwidth',
        'nico_RaytracingVk'
    )
    AND array_position(supported_platforms, '7d9ecto1ww') IS NULL;

UPDATE
    machine_validation_tests
SET
    pre_condition = '/opt/forge/benchpress-fio-file-pre-setup.sh',
    extra_output_file = '/opt/benchpress/results/fio_file_stdout.txt',
    extra_err_file = '/opt/benchpress/results/fio_file_stderr.txt',
    command = '/opt/benchpress/benchpress',
    img_name = null,
    container_arg = null
where
    test_id = 'nico_FioFile';