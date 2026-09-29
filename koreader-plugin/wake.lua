local util = require("util")

local Wake = {}
Wake.service = "koreader-rss-wake"
Wake.config = "/etc/upstart/koreader-rss-wake.conf"
Wake.supervisor = "/mnt/us/koreader/data/rssreader/rss-wake-supervisor.sh"
Wake.version = 5

local function absolutePath(path)
    if path:sub(1, 1) == "/" then return path end
    local pipe = io.popen("pwd -P", "r")
    if not pipe then return nil end
    local cwd = pipe:read("*l")
    pipe:close()
    if not cwd or cwd:sub(1, 1) ~= "/" then return nil end
    return cwd .. "/" .. path
end

local function replacement(value)
    return (value:gsub("%%", "%%%%"))
end

local function exists(path)
    local handle = io.open(path, "r")
    if handle then handle:close(); return true end
    return false
end

local function command(arguments)
    local line = util.shell_escape(arguments) .. " >/dev/null 2>&1"
    local ok, _, code = os.execute(line)
    return ok == true or ok == 0 or code == 0
end

local function rootWritable()
    return command({ "mntroot", "rw" })
end

local function rootReadonly()
    return command({ "mntroot", "ro" })
end

function Wake.isSupported(backend)
    return exists("/usr/bin/lipc-wait-event") and command({ "lipc-get-prop", "com.lab126.powerd", "state" })
        and command({ "which", "start" }) and command({ "which", "stop" })
        and command({ "which", "status" }) and exists("/etc/upstart") and exists(backend)
end

function Wake.isInstalled()
    return exists(Wake.config)
end

function Wake.isRunning()
    return command({ "status", Wake.service })
end

function Wake.start()
    return command({ "start", Wake.service })
end

function Wake.status(backend)
    local installed = Wake.isInstalled()
    local version
    if installed then
        local file = io.open(Wake.config, "r")
        local text = file and file:read("*a") or ""
        if file then file:close() end
        version = tonumber(text:match("version:%s*(%d+)"))
    end
    return {
        supported = Wake.isSupported(backend),
        installed = installed,
        running = installed and Wake.isRunning() or false,
        version = version,
    }
end

function Wake.install(plugin_path, backend, database, budget)
    plugin_path = absolutePath(plugin_path)
    backend = absolutePath(backend)
    database = absolutePath(database)
    if not plugin_path or not backend or not database then
        return nil, "could not resolve absolute wake service paths"
    end
    if not Wake.isSupported(backend) then return nil, "wake integration is not supported" end
    local template = plugin_path .. "/resources/koreader-rss-wake.conf"
    local supervisor_template = plugin_path .. "/resources/rss-wake-supervisor.sh"
    local supervisor = database:gsub("/[^/]+$", "/rss-wake-supervisor.sh")
    if not exists(template) then return nil, "wake template is missing" end
    if not exists(supervisor_template) then return nil, "wake supervisor is missing" end
    if not rootWritable() then return nil, "could not make system root writable" end
    local input = io.open(template, "r")
    local text = input and input:read("*a")
    if input then input:close() end
    local supervisor_input = io.open(supervisor_template, "r")
    local supervisor_text = supervisor_input and supervisor_input:read("*a")
    if supervisor_input then supervisor_input:close() end
    local ok = text ~= nil
    if ok and supervisor_text then
        supervisor_text = supervisor_text:gsub("@BACKEND@", replacement(backend))
            :gsub("@DATABASE@", replacement(database)):gsub("@BUDGET@", tostring(budget or 600))
        text = text:gsub("@SUPERVISOR@", replacement(supervisor))
        local supervisor_output = io.open(supervisor .. ".new", "w")
        ok = supervisor_output ~= nil
        if supervisor_output then supervisor_output:write(supervisor_text); supervisor_output:close() end
        if ok then ok = os.rename(supervisor .. ".new", supervisor) end
        if ok then ok = command({ "chmod", "755", supervisor }) end
    else
        ok = false
    end
    if ok then
        local output = io.open(Wake.config .. ".new", "w")
        ok = output ~= nil
        if output then output:write(text); output:close() end
    end
    if ok then ok = os.rename(Wake.config .. ".new", Wake.config) end
    local restored = rootReadonly()
    if not restored then return nil, "installed wake service but could not restore read-only root" end
    if not ok then return nil, "could not install wake service" end
    if not command({ "start", Wake.service }) then return nil, "could not start wake service" end
    return true
end

function Wake.uninstall()
    command({ "stop", Wake.service })
    if not Wake.isInstalled() then return true end
    if not rootWritable() then return nil, "could not make system root writable" end
    local ok = os.remove(Wake.config) == true
    os.remove(Wake.supervisor)
    local restored = rootReadonly()
    if not restored then return nil, "removed wake service but could not restore read-only root" end
    return ok or not exists(Wake.config), ok and nil or "could not remove wake service"
end

return Wake
