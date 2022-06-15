// This file is part of SubQuery.

// Copyright (C) 2020-2022 SubQuery Pte Ltd authors & contributors
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

pub const APPLICATION_JSON: &str = "application/json";

pub const KEEP_ALIVE: &str = "Keep-Alive";

pub const AUTHORIZATION: &str = "Authorization";

pub const HEADERS: [&'static str; 5] = [
    "content-type",
    "x-apollo-tracing",
    "agent",
    "authorization",
    "user-agent",
];
