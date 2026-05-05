UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, 'thinksystem_sr675_v3_ovx')
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
    AND array_position(supported_platforms, 'thinksystem_sr675_v3_ovx') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '7d9rcto1ww')
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
    AND array_position(supported_platforms, '7d9rcto1ww') IS NULL;

UPDATE
    machine_validation_tests
SET
    supported_platforms = array_append(supported_platforms, '7d9acto1ww')
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
    AND array_position(supported_platforms, '7d9acto1ww') IS NULL;

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