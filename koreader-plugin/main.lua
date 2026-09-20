local DataStorage = require("datastorage")
local Device = require("device")
local InfoMessage = require("ui/widget/infomessage")
local InputDialog = require("ui/widget/inputdialog")
local ConfirmBox = require("ui/widget/confirmbox")
local Menu = require("ui/widget/menu")
local NetworkMgr = require("ui/network/manager")
local ReaderUI = require("apps/reader/readerui")
local Trapper = require("ui/trapper")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local util = require("util")
local _ = require("gettext")
local Wake = require("wake")

local RSSReader = WidgetContainer:extend{
    name = "rssreader",
    is_doc_only = false,
}

local function show(text)
    UIManager:show(InfoMessage:new{ text = text })
end

local function trim(text)
    return (text or ""):match("^%s*(.-)%s*$")
end

local function withDatabase(path, callback)
    local connection
    local ok, result = pcall(function()
        connection = require("lua-ljsqlite3/init").open(path)
        return callback(connection)
    end)
    if connection then pcall(connection.close, connection) end
    return ok, result
end

local function queryRows(path, sql, map)
    return withDatabase(path, function(connection)
        local statement = connection:prepare(sql)
        local items = {}
        local ok, err = pcall(function()
            while true do
                local row = statement:step()
                if not row then break end
                items[#items + 1] = map(row)
            end
        end)
        pcall(statement.close, statement)
        if not ok then error(err) end
        return items
    end)
end

function RSSReader:init()
    local root = DataStorage:getDataDir()
    self.backend = self.path .. "/bin/rss-backend"
    self.data_dir = root .. "/data/rssreader"
    self.cache_dir = root .. "/cache/rssreader"
    self.database = self.data_dir .. "/rss.sqlite3"
    self.probe_database = self.data_dir .. "/probe.sqlite3"
    self.fixture = self.cache_dir .. "/integration-probe.html"
    self.ui.menu:registerToMainMenu(self)
    self:importOpmlFeeds()
end

function RSSReader:importOpmlFeeds()
    self:runBackend({ self.backend, "--db", self.database, "feed", "import-opml", self.path .. "/feeds" },
        _("Importing OPML feeds…"), function(output)
            local imported = tonumber(output:match("imported=(%d+)") or "0") or 0
            if imported > 0 then show(string.format(_("Imported %d feeds."), imported)) end
        end)
end

function RSSReader:showAllArticles(feed_id, title, offset)
    offset = offset or 0
    local sql = "SELECT a.id,a.title,COALESCE(f.title,f.source_url),a.sort_at,a.url FROM articles a JOIN feeds f ON f.id=a.feed_id"
    if feed_id then sql = sql .. " WHERE a.feed_id = " .. tostring(feed_id) end
    sql = sql .. " ORDER BY a.sort_at DESC,a.id DESC LIMIT 100 OFFSET " .. tostring(offset)
    local ok, result = queryRows(self.database, sql, function(row)
        return {
            article_id = tonumber(row[1]),
            text = tostring(row[2]) .. "\n" .. string.format("%s · %s", tostring(row[3]), os.date("%Y-%m-%d", tonumber(row[4]))),
            multilines_forced = true,
            url = row[5],
        }
    end)
    if not ok then show(tostring(result)); return end
    if #result == 0 then show(_("No articles.")); return end
    if #result == 100 then
        result[#result + 1] = { text = _("Next page →"), next_offset = offset + 100 }
    end
    local menu = Menu:new{
        title = title or _("Latest articles"), item_table = result, covers_fullscreen = true,
        multilines_forced = true,
        items_max_lines = 2,
        onMenuSelect = function(_, item)
            if item.next_offset then
                UIManager:close(menu)
                self:showAllArticles(feed_id, title, item.next_offset)
            else
                UIManager:close(menu)
                self:openArticle(item.article_id)
            end
        end,
        onMenuHold = function(_, item)
            if item.url and item.url ~= "" then Device:openLink(item.url) end
        end,
    }
    UIManager:show(menu)
end

function RSSReader:showStatus()
    self:runBackend({ self.backend, "--db", self.database, "status" }, _("Loading refresh status…"), function(output)
        output = output:gsub("started_at=(%-?%d+)", function(value)
            return "started_at=" .. os.date("!%Y-%m-%d %H:%M:%S UTC", tonumber(value))
        end)
        output = output:gsub("finished_at=(%-?%d+)", function(value)
            local timestamp = tonumber(value)
            return "finished_at=" .. (timestamp == 0 and _("not finished") or os.date("!%Y-%m-%d %H:%M:%S UTC", timestamp))
        end)
        show(output)
    end)
end

function RSSReader:refreshNow()
    self:runBackend({ self.backend, "--db", self.database, "refresh", "--reason", "manual" },
        _("Refreshing feeds…"), function() self:showStatus() end)
end

function RSSReader:showWakeStatus()
    local status = Wake.status(self.backend)
    local support = status.supported and _("supported") or _("unsupported")
    local state = status.running and _("running") or (status.installed and _("stopped") or _("not installed"))
    show(string.format(
        _("Wake refresh: %s\nService: %s\nConfiguration version: %s"),
        support, state, tostring(status.version or _("none"))
    ))
end

function RSSReader:enableWake()
    local status = Wake.status(self.backend)
    if not status.supported then
        show(_("Wake refresh is not supported on this device."))
        return
    end
    if status.installed then
        show(_("Wake refresh is already enabled."))
        return
    end
    local ok, err = Wake.install(self.path, self.backend, self.database, 30)
    show(ok and _("Wake refresh enabled.") or tostring(err))
end

function RSSReader:disableWake()
    local status = Wake.status(self.backend)
    if not status.installed then
        show(_("Wake refresh is already disabled."))
        return
    end
    local ok, err = Wake.uninstall()
    show(ok and _("Wake refresh disabled.") or tostring(err))
end

function RSSReader:showWake()
    local actions = Menu:new{
        title = _("Wake refresh"),
        item_table = {
            { text = _("Check status"), callback = function() UIManager:close(actions); self:showWakeStatus() end },
            { text = _("Enable"), callback = function() UIManager:close(actions); self:enableWake() end },
            { text = _("Disable"), callback = function() UIManager:close(actions); self:disableWake() end },
        },
        covers_fullscreen = true,
    }
    UIManager:show(actions)
end

function RSSReader:showFeeds()
    self:ensureDirectories()
    self:runBackend({ self.backend, "--db", self.database, "feed", "list" },
        _("Loading feeds…"), function(output)
            local items = {}
            for line in (output .. "\n"):gmatch("([^\n]*)\n") do
                local id, enabled, title, url, failures, last_error = line:match("^(%d+)\t([^\t]*)\t([^\t]*)\t([^\t]*)\t(%d+)\t(.*)$")
                if id then
                    local is_enabled = enabled == "enabled"
                    local label = (title ~= "" and title or url)
                    if tonumber(failures) > 0 and last_error ~= "" then label = label .. " [!] " .. last_error end
                    table.insert(items, { feed_id = tonumber(id), enabled = is_enabled, text = (is_enabled and "[on] " or "[off] ") .. label, mandatory = url })
                end
            end
            if #items == 0 then show(_("No feeds configured.")); return end
            local menu
            menu = Menu:new{
                title = _("Feeds"), item_table = items, covers_fullscreen = true,
                onMenuSelect = function(_, item) self:showFeedActions(item, menu) end,
                onMenuHold = function(_, item) self:toggleFeed(item, menu) end,
            }
            UIManager:show(menu)
        end)
end

function RSSReader:showFeedActions(item, feeds_menu)
    local actions = Menu:new{
        title = item.text,
        item_table = {
            { text = _("Articles"), callback = function()
                UIManager:close(actions); UIManager:close(feeds_menu)
                self:showAllArticles(item.feed_id, item.text)
            end },
            { text = item.enabled and _("Disable feed") or _("Enable feed"), callback = function()
                UIManager:close(actions); self:toggleFeed(item, feeds_menu)
            end },
            { text = _("Remove feed"), callback = function()
                UIManager:close(actions); self:confirmRemoveFeed(item.feed_id, item.text, feeds_menu)
            end },
        },
        covers_fullscreen = true,
    }
    UIManager:show(actions)
end

function RSSReader:toggleFeed(item, menu)
    local command = item.enabled and "disable" or "enable"
    self:runBackend({ self.backend, "--db", self.database, "feed", command, tostring(item.feed_id) },
        item.enabled and _("Disabling feed…") or _("Enabling feed…"), function()
            UIManager:close(menu)
            self:showFeeds()
        end)
end

function RSSReader:confirmRemoveFeed(feed_id, title, menu)
    UIManager:show(ConfirmBox:new{
        text = string.format(_("Remove feed '%s'?"), title),
        ok_text = _("Remove"),
        ok_callback = function()
            self:runBackend({ self.backend, "--db", self.database, "feed", "remove", tostring(feed_id) },
                _("Removing feed…"), function() UIManager:close(menu); self:showFeeds() end)
        end,
    })
end

function RSSReader:confirmPurge(kind)
    local is_feeds = kind == "feeds"
    UIManager:show(ConfirmBox:new{
        text = is_feeds and _("Remove ALL feeds and their articles?") or _("Remove ALL stored articles?"),
        ok_text = _("Remove all"),
        ok_callback = function()
            local command = is_feeds and { "feed", "remove-all" } or { "articles", "remove-all" }
            self:runBackend({ self.backend, "--db", self.database, command[1], command[2] },
                is_feeds and _("Removing all feeds…") or _("Removing all articles…"),
                function(output) show(output:gsub("\n", " ")) end)
        end,
    })
end

function RSSReader:addFeedDialog()
    local dialog
    local function submit()
        local url = trim(dialog:getInputText())
        if url == "" or url == "https://" then
            show(_("Enter a feed URL."))
            return
        end
        UIManager:close(dialog)
        self:runBackend({ self.backend, "--db", self.database, "feed", "check", url },
            _("Checking feed…"), function(output)
                local title = output:match("title=(.*)$") or ""
                self:runBackend({ self.backend, "--db", self.database, "feed", "add", url, title },
                    _("Adding feed…"), function() show(_("Feed added.")) end)
            end)
    end
    dialog = InputDialog:new{
        title = _("Add feed URL"), input = "https://",
        buttons = {{
            {
                text = _("Cancel"), id = "close",
                callback = function() UIManager:close(dialog) end,
            },
            {
                text = _("Add"), is_enter_default = true,
                callback = submit,
            },
        }},
    }
    UIManager:show(dialog)
end

function RSSReader:openArticle(article_id)
    local ok, err = self:ensureDirectories()
    if not ok then show(tostring(err)); return end
    self:runBackend({ self.backend, "--db", self.database, "materialize", tostring(article_id), "--cache", self.cache_dir },
        _("Opening article…"), function(output)
            local path = trim(output):match("([^\r\n]+)$")
            if not path or path == "" then show(_("Backend returned no article path.")); return end
            ReaderUI:showReader(path, nil, nil, nil, function()
                -- Article state is intentionally not persisted.
            end)
        end)
end

function RSSReader:ensureDirectories()
    local ok, err = util.makePath(self.data_dir)
    if not ok then return nil, err end
    ok, err = util.makePath(self.cache_dir)
    if not ok then return nil, err end
    return true
end

function RSSReader:runBackend(arguments, title, callback)
    local command = util.shell_escape(arguments)
        .. " 2>&1; status=$?; printf '\\n__RSS_EXIT=%s\\n' \"$status\""
    Trapper:wrap(function()
        local completed, output = Trapper:dismissablePopen(command, title)
        if not completed then
            show(_("RSS backend operation dismissed; the bounded process may finish in the background."))
            return
        end
        local status = output and output:match("__RSS_EXIT=(%d+)%s*$")
        local body = trim((output or ""):gsub("%s*__RSS_EXIT=%d+%s*$", ""))
        if status ~= "0" then
            show(_("RSS backend failed:") .. "\n" .. (body ~= "" and body or _("No diagnostic output")))
            return
        end
        callback(body)
    end)
end

function RSSReader:showEnvironment()
    local Screen = Device.screen
    show(string.format(
        "Screen: %d × %d\nData: %s\nCache: %s\nPlugin: %s\nBackend: %s",
        Screen:getWidth(), Screen:getHeight(), DataStorage:getDataDir(),
        self.cache_dir, self.path, self.backend
    ))
end

function RSSReader:runDoctor()
    self:runBackend({ self.backend, "doctor" }, _("Running RSS backend doctor…"), function(output)
        show(output)
    end)
end

function RSSReader:runHttpsProbe()
    NetworkMgr:runWhenOnline(function()
        self:runBackend(
            { self.backend, "http-probe", "--url", "https://example.com/" },
            _("Testing HTTPS and certificate roots…"),
            function(output) show(output) end
        )
    end)
end

function RSSReader:queryProbeDatabase()
    local ok, result = withDatabase(self.probe_database, function(connection)
        local sqlite_version = connection:rowexec("SELECT sqlite_version()")
        local value = connection:rowexec("SELECT value FROM meta WHERE key='probe'")
        local journal = connection:rowexec("PRAGMA journal_mode")
        return string.format(
            "Lua SQLite: %s\nProbe value: %s\nJournal: %s\nDatabase: %s",
            tostring(sqlite_version), tostring(value), tostring(journal), self.probe_database
        )
    end)
    if ok then
        show(result)
    else
        show(_("SQLite probe failed:") .. "\n" .. tostring(result))
    end
end

function RSSReader:initializeAndQueryDatabase()
    local ok, err = self:ensureDirectories()
    if not ok then
        show(_("Cannot create RSS data directory:") .. "\n" .. tostring(err))
        return
    end
    self:runBackend(
        { self.backend, "init-probe-db", "--db", self.probe_database },
        _("Creating SQLite integration probe…"),
        function() self:queryProbeDatabase() end
    )
end

function RSSReader:materializeAndOpenFixture()
    local ok, err = self:ensureDirectories()
    if not ok then
        show(_("Cannot create RSS cache directory:") .. "\n" .. tostring(err))
        return
    end
    self:runBackend(
        { self.backend, "materialize-fixture", "--out", self.fixture },
        _("Materializing offline HTML fixture…"),
        function(output)
            local path = output:match("([^\r\n]+)%s*$")
            if not path or path == "" then
                show(_("Backend returned no fixture path."))
                return
            end
            ReaderUI:showReader(path, nil, nil, nil, function()
                show(_("Offline HTML fixture opened successfully."))
            end)
        end
    )
end

function RSSReader:addToMainMenu(menu_items)
    menu_items.rss_reader_probe = {
        text = _("RSS Reader"),
        sub_item_table = {
            {
                text = _("Latest articles"),
                callback = function() self:showAllArticles() end,
            },
            {
                text = _("Refresh now"),
                callback = function() self:refreshNow() end,
            },
            {
                text = _("Refresh status"),
                callback = function() self:showStatus() end,
            },
            {
                text = _("Wake refresh"),
                callback = function() self:showWake() end,
            },
            {
                text = _("Feeds"),
                callback = function() self:showFeeds() end,
            },
            {
                text = _("Add feed"),
                callback = function() self:addFeedDialog() end,
            },
            {
                text = _("Remove all articles"),
                callback = function() self:confirmPurge("articles") end,
            },
            {
                text = _("Remove all feeds"),
                callback = function() self:confirmPurge("feeds") end,
            },
            {
                text = _("Environment and paths"),
                callback = function() self:showEnvironment() end,
            },
            {
                text = _("Run backend doctor"),
                callback = function() self:runDoctor() end,
            },
            {
                text = _("Test HTTPS and certificates"),
                callback = function() self:runHttpsProbe() end,
            },
            {
                text = _("Initialize and query SQLite"),
                callback = function() self:initializeAndQueryDatabase() end,
            },
            {
                text = _("Open offline HTML fixture"),
                callback = function() self:materializeAndOpenFixture() end,
            },
        },
    }
end

return RSSReader
