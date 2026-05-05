/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

// File created from nico_logger.mes

#include <cstddef>
#include <log/message_types.h>
#include <log/message_initializer.h>

namespace isc {
namespace log {

extern const isc::log::MessageID LOG_NICO_GENERIC = "LOG_NICO_GENERIC";
extern const isc::log::MessageID LOG_NICO_INITIALIZATION = "LOG_NICO_INITIALIZATION";
extern const isc::log::MessageID LOG_NICO_INVALID_HANDLE = "LOG_NICO_INVALID_HANDLE";
extern const isc::log::MessageID LOG_NICO_INVALID_NEXTSERVER_IPV4 = "LOG_NICO_INVALID_NEXTSERVER_IPV4";
extern const isc::log::MessageID LOG_NICO_LEASE4_SELECT = "LOG_NICO_LEASE4_SELECT";
extern const isc::log::MessageID LOG_NICO_LEASE_EXPIRE = "LOG_NICO_LEASE_EXPIRE";
extern const isc::log::MessageID LOG_NICO_LEASE_EXPIRE_ERROR = "LOG_NICO_LEASE_EXPIRE_ERROR";
extern const isc::log::MessageID LOG_NICO_PKT4_RECEIVE = "LOG_NICO_PKT4_RECEIVE";
extern const isc::log::MessageID LOG_NICO_PKT4_SEND = "LOG_NICO_PKT4_SEND";

} // namespace log
} // namespace isc

namespace {

const char* values[] = {
    "LOG_NICO_GENERIC", "Nico: %1",
    "LOG_NICO_INITIALIZATION", "Nico Kea shim loading",
    "LOG_NICO_INVALID_HANDLE", "Nico hook shim_load() was called with an invalid LibraryHandle",
    "LOG_NICO_INVALID_NEXTSERVER_IPV4", "Invalid provisioning server IPv4 address: %1",
    "LOG_NICO_LEASE4_SELECT", "Nico hook called for DHCPv4 lease selected from %1",
    "LOG_NICO_LEASE_EXPIRE", "Nico releasing expired DHCP lease for %1",
    "LOG_NICO_LEASE_EXPIRE_ERROR", "Nico failed to release expired DHCP lease for %1",
    "LOG_NICO_PKT4_RECEIVE", "Nico hook called for DHCPv4 packet receive from %1",
    "LOG_NICO_PKT4_SEND", "Nico hook called for DHCPv4 packet send from %1",
    NULL
};

const isc::log::MessageInitializer initializer(values);

} // Anonymous namespace

