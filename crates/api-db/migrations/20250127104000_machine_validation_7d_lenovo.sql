UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '7d9rctolww')
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
    AND array_position(supported_platforms, '7d9rctolww') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '7d9actolww')
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
    AND array_position(supported_platforms, '7d9actolww') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '7d9ectolww')
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
    AND array_position(supported_platforms, '7d9ectolww') IS NULL;