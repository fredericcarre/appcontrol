//! Technology pattern matching for automatic discovery enrichment.
//!
//! This module identifies well-known technologies from process name, command line,
//! and listening ports. Focus on enterprise/banking technologies:
//! - Middleware: IBM MQ, TIBCO, WebLogic, WebSphere, Tuxedo
//! - Databases: Oracle, SQL Server, DB2, Sybase, PostgreSQL, MySQL
//! - Schedulers: Control-M, AutoSys, Dollar Universe, TWS
//! - File Transfer: Connect:Direct, Axway
//! - Standard: ElasticSearch, RabbitMQ, Kafka, nginx, etc.

use appcontrol_common::{CommandSuggestion, TechnologyHint};

/// A pattern that identifies a technology from process attributes.
pub struct TechPattern {
    pub id: &'static str,
    pub display_name: &'static str,
    pub icon: &'static str,
    pub layer: &'static str,
    pub process_names: &'static [&'static str],
    pub cmdline_patterns: &'static [&'static str],
    pub port_hints: &'static [u16],
    pub windows_commands: Option<TechCommands>,
    pub linux_commands: Option<TechCommands>,
}

pub struct TechCommands {
    pub check: &'static str,
    pub start: Option<&'static str>,
    pub stop: Option<&'static str>,
    pub restart: Option<&'static str>,
    pub logs: Option<&'static str>,
    pub version: Option<&'static str>,
}

pub static TECH_PATTERNS: &[TechPattern] = &[
    // =========================================================================
    // DATABASES - ENTERPRISE
    // =========================================================================
    TechPattern {
        id: "oracle",
        display_name: "Oracle Database",
        icon: "oracle",
        layer: "Database",
        process_names: &["oracle", "ora_pmon", "ora_smon", "tnslsnr"],
        cmdline_patterns: &["ora_pmon", "ora_smon", "tnslsnr", "oracle"],
        port_hints: &[1521, 1522, 1523],
        windows_commands: Some(TechCommands {
            check: r#"sc query OracleService* | findstr RUNNING"#,
            start: Some("net start OracleServiceORCL"),
            stop: Some("net stop OracleServiceORCL"),
            restart: None,
            logs: Some(r#"type "%ORACLE_HOME%\diag\rdbms\*\*\trace\alert_*.log" | more"#),
            version: Some("sqlplus -version"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep ora_pmon | grep -v grep",
            start: Some("sqlplus / as sysdba <<< 'startup'"),
            stop: Some("sqlplus / as sysdba <<< 'shutdown immediate'"),
            restart: None,
            logs: Some("tail -100 $ORACLE_BASE/diag/rdbms/*/*/trace/alert_*.log"),
            version: Some("sqlplus -version"),
        }),
    },
    TechPattern {
        id: "sqlserver",
        display_name: "SQL Server",
        icon: "sqlserver",
        layer: "Database",
        process_names: &["sqlservr.exe", "sqlservr"],
        cmdline_patterns: &["sqlservr", "MSSQLSERVER"],
        port_hints: &[1433, 1434],
        windows_commands: Some(TechCommands {
            check: r#"sc query MSSQLSERVER | findstr RUNNING"#,
            start: Some("net start MSSQLSERVER"),
            stop: Some("net stop MSSQLSERVER"),
            restart: Some("net stop MSSQLSERVER && net start MSSQLSERVER"),
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "C:\Program Files\Microsoft SQL Server\MSSQL*\MSSQL\Log\ERRORLOG""#),
            version: Some(r#"sqlcmd -Q "SELECT @@VERSION""#),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active mssql-server",
            start: Some("systemctl start mssql-server"),
            stop: Some("systemctl stop mssql-server"),
            restart: Some("systemctl restart mssql-server"),
            logs: Some("tail -100 /var/opt/mssql/log/errorlog"),
            version: Some(r#"/opt/mssql-tools/bin/sqlcmd -Q "SELECT @@VERSION""#),
        }),
    },
    TechPattern {
        id: "db2",
        display_name: "IBM DB2",
        icon: "db2",
        layer: "Database",
        process_names: &["db2sysc", "db2syscs", "db2fmp"],
        cmdline_patterns: &["db2sysc", "db2start", "db2inst"],
        port_hints: &[50000, 50001],
        windows_commands: Some(TechCommands {
            check: r#"db2 get instance | findstr /I "instance""#,
            start: Some("db2start"),
            stop: Some("db2stop force"),
            restart: Some("db2stop force && db2start"),
            logs: Some(r#"type "%DB2PATH%\DB2\*\db2diag.log" | more"#),
            version: Some("db2level"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep db2sysc | grep -v grep",
            start: Some("db2start"),
            stop: Some("db2stop force"),
            restart: Some("db2stop force && db2start"),
            logs: Some("tail -100 ~/sqllib/db2dump/db2diag.log"),
            version: Some("db2level"),
        }),
    },
    TechPattern {
        id: "sybase",
        display_name: "SAP ASE (Sybase)",
        icon: "sybase",
        layer: "Database",
        process_names: &["dataserver", "backupserver"],
        cmdline_patterns: &["dataserver", "SYBASE", "ASE"],
        port_hints: &[5000, 5001],
        windows_commands: Some(TechCommands {
            check: r#"sc query SYBSQL_* | findstr RUNNING"#,
            start: Some("net start SYBSQL_ASE"),
            stop: Some("net stop SYBSQL_ASE"),
            restart: None,
            logs: Some(r#"type "%SYBASE%\ASE-*\*.log" | more"#),
            version: Some("isql -v"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep dataserver | grep -v grep",
            start: Some("startserver -f $SYBASE/$SYBASE_ASE/install/RUN_*"),
            stop: Some("isql -Usa -P -S$DSQUERY <<< 'shutdown'"),
            restart: None,
            logs: Some("tail -100 $SYBASE/$SYBASE_ASE/install/*.log"),
            version: Some("isql -v"),
        }),
    },
    TechPattern {
        id: "mysql",
        display_name: "MySQL",
        icon: "mysql",
        layer: "Database",
        process_names: &["mysqld", "mysqld.exe"],
        cmdline_patterns: &["mysqld"],
        port_hints: &[3306],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "Caption Like 'mysqld.exe'" | findstr mysqld.exe"#,
            start: Some("sc start MySQL"),
            stop: Some("sc stop MySQL"),
            restart: Some("sc stop MySQL && sc start MySQL"),
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "C:\ProgramData\MySQL\MySQL Server*\Data\*.err""#),
            version: Some("mysql --version"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active mysql || systemctl is-active mysqld",
            start: Some("systemctl start mysql"),
            stop: Some("systemctl stop mysql"),
            restart: Some("systemctl restart mysql"),
            logs: Some("journalctl -u mysql -n 100 --no-pager"),
            version: Some("mysql --version"),
        }),
    },
    TechPattern {
        id: "postgresql",
        display_name: "PostgreSQL",
        icon: "postgresql",
        layer: "Database",
        process_names: &["postgres", "postmaster"],
        cmdline_patterns: &["postgres", "postmaster"],
        port_hints: &[5432],
        windows_commands: Some(TechCommands {
            check: r#"sc query postgresql-* | findstr RUNNING"#,
            start: Some("net start postgresql-*"),
            stop: Some("net stop postgresql-*"),
            restart: None,
            logs: Some(r#"type "C:\Program Files\PostgreSQL\*\data\log\*.log" | more"#),
            version: Some("psql --version"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active postgresql",
            start: Some("systemctl start postgresql"),
            stop: Some("systemctl stop postgresql"),
            restart: Some("systemctl restart postgresql"),
            logs: Some("journalctl -u postgresql -n 100 --no-pager"),
            version: Some("psql --version"),
        }),
    },
    TechPattern {
        id: "elasticsearch",
        display_name: "ElasticSearch",
        icon: "elastic",
        layer: "Database",
        process_names: &["elasticsearch"],
        cmdline_patterns: &["org.elasticsearch.bootstrap.Elasticsearch", "elasticsearch"],
        port_hints: &[9200, 9300],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%org.elasticsearch.bootstrap.Elasticsearch%' and Caption = 'java.exe'" get caption | findstr java.exe"#,
            start: Some(r#"{INSTALL_DIR}\bin\elasticsearch.bat >NUL 2>&1"#),
            stop: Some(r#"wmic Path win32_process Where "CommandLine Like '%org.elasticsearch.bootstrap.Elasticsearch%' and Caption = 'java.exe'" Call Terminate"#),
            restart: None,
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "{INSTALL_DIR}\logs\elasticsearch.log""#),
            version: Some(r#"cd /D {INSTALL_DIR}\bin && elasticsearch --version"#),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active elasticsearch",
            start: Some("systemctl start elasticsearch"),
            stop: Some("systemctl stop elasticsearch"),
            restart: Some("systemctl restart elasticsearch"),
            logs: Some("journalctl -u elasticsearch -n 100 --no-pager"),
            version: Some("elasticsearch --version"),
        }),
    },
    TechPattern {
        id: "mongodb",
        display_name: "MongoDB",
        icon: "mongodb",
        layer: "Database",
        process_names: &["mongod", "mongod.exe"],
        cmdline_patterns: &["mongod"],
        port_hints: &[27017],
        windows_commands: Some(TechCommands {
            check: r#"sc query MongoDB | findstr RUNNING"#,
            start: Some("net start MongoDB"),
            stop: Some("net stop MongoDB"),
            restart: None,
            logs: Some(r#"type "C:\Program Files\MongoDB\Server\*\log\mongod.log" | more"#),
            version: Some("mongod --version"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active mongod",
            start: Some("systemctl start mongod"),
            stop: Some("systemctl stop mongod"),
            restart: Some("systemctl restart mongod"),
            logs: Some("journalctl -u mongod -n 100 --no-pager"),
            version: Some("mongod --version"),
        }),
    },
    TechPattern {
        id: "redis",
        display_name: "Redis",
        icon: "redis",
        layer: "Database",
        process_names: &["redis-server", "redis-server.exe"],
        cmdline_patterns: &["redis-server"],
        port_hints: &[6379],
        windows_commands: Some(TechCommands {
            check: r#"sc query Redis | findstr RUNNING"#,
            start: Some("net start Redis"),
            stop: Some("net stop Redis"),
            restart: None,
            logs: None,
            version: Some("redis-server --version"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active redis",
            start: Some("systemctl start redis"),
            stop: Some("systemctl stop redis"),
            restart: Some("systemctl restart redis"),
            logs: Some("journalctl -u redis -n 100 --no-pager"),
            version: Some("redis-server --version"),
        }),
    },

    // =========================================================================
    // MIDDLEWARE - IBM MQ (WebSphere MQ)
    // =========================================================================
    TechPattern {
        id: "ibmmq",
        display_name: "IBM MQ",
        icon: "ibmmq",
        layer: "Middleware",
        process_names: &["amqzxma0", "runmqsc", "amqpcsea.exe", "amqsvc.exe"],
        cmdline_patterns: &["amqzxma", "runmqsc", "QMGR", "IBM\\MQ", "IBM/MQ", "WebSphere MQ"],
        port_hints: &[1414, 1415],
        windows_commands: Some(TechCommands {
            check: r#"dspmq -m {QMGR} | findstr Running"#,
            start: Some("strmqm {QMGR}"),
            stop: Some("endmqm -i {QMGR}"),
            restart: Some("endmqm -i {QMGR} && strmqm {QMGR}"),
            logs: Some(r#"type "C:\ProgramData\IBM\MQ\qmgrs\{QMGR}\errors\AMQERR01.LOG" | more"#),
            version: Some("dspmqver"),
        }),
        linux_commands: Some(TechCommands {
            check: "dspmq -m {QMGR} | grep Running",
            start: Some("strmqm {QMGR}"),
            stop: Some("endmqm -i {QMGR}"),
            restart: Some("endmqm -i {QMGR} && strmqm {QMGR}"),
            logs: Some("tail -100 /var/mqm/qmgrs/{QMGR}/errors/AMQERR01.LOG"),
            version: Some("dspmqver"),
        }),
    },

    // =========================================================================
    // MIDDLEWARE - TIBCO
    // =========================================================================
    TechPattern {
        id: "tibcoems",
        display_name: "TIBCO EMS",
        icon: "tibco",
        layer: "Middleware",
        process_names: &["tibemsd", "tibemsd64", "tibemsd.exe"],
        cmdline_patterns: &["tibemsd", "TIBCO/ems"],
        port_hints: &[7222, 7243],
        windows_commands: Some(TechCommands {
            check: r#"sc query "TIBCO EMS*" | findstr RUNNING"#,
            start: Some(r#"net start "TIBCO EMS""#),
            stop: Some(r#"net stop "TIBCO EMS""#),
            restart: None,
            logs: Some(r#"type "%TIBCO_HOME%\ems\*\logs\*.log" | more"#),
            version: Some("tibemsd -version"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep tibemsd | grep -v grep",
            start: Some("tibemsd -config $TIBCO_HOME/ems/bin/tibemsd.conf &"),
            stop: Some("pkill tibemsd"),
            restart: None,
            logs: Some("tail -100 $TIBCO_HOME/ems/logs/*.log"),
            version: Some("tibemsd -version"),
        }),
    },
    TechPattern {
        id: "tibcobw",
        display_name: "TIBCO BusinessWorks",
        icon: "tibco",
        layer: "Application",
        process_names: &["bwengine", "bwengine.exe"],
        cmdline_patterns: &["bwengine", "TIBCO/bw", "BusinessWorks"],
        port_hints: &[],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%bwengine%'" | findstr bwengine"#,
            start: None,
            stop: Some(r#"wmic Path win32_process Where "CommandLine Like '%bwengine%'" Call Terminate"#),
            restart: None,
            logs: Some(r#"type "%TIBCO_HOME%\bw\*\logs\*.log" | more"#),
            version: None,
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep bwengine | grep -v grep",
            start: None,
            stop: Some("pkill -f bwengine"),
            restart: None,
            logs: Some("tail -100 $TIBCO_HOME/bw/logs/*.log"),
            version: None,
        }),
    },

    // =========================================================================
    // MIDDLEWARE - ORACLE
    // =========================================================================
    TechPattern {
        id: "weblogic",
        display_name: "Oracle WebLogic",
        icon: "weblogic",
        layer: "Application",
        process_names: &[],
        cmdline_patterns: &["weblogic.Server", "weblogic.NodeManager", "wlserver", "bea.home"],
        port_hints: &[7001, 7002, 9001, 9002],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%weblogic.Server%'" | findstr java"#,
            start: Some(r#"{DOMAIN_HOME}\bin\startWebLogic.cmd"#),
            stop: Some(r#"{DOMAIN_HOME}\bin\stopWebLogic.cmd"#),
            restart: None,
            logs: Some(r#"type "{DOMAIN_HOME}\servers\{SERVER}\logs\{SERVER}.log" | more"#),
            version: Some(r#"java weblogic.version"#),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep 'weblogic.Server' | grep -v grep",
            start: Some("$DOMAIN_HOME/bin/startWebLogic.sh &"),
            stop: Some("$DOMAIN_HOME/bin/stopWebLogic.sh"),
            restart: None,
            logs: Some("tail -100 $DOMAIN_HOME/servers/$SERVER/logs/$SERVER.log"),
            version: Some("java weblogic.version"),
        }),
    },
    TechPattern {
        id: "tuxedo",
        display_name: "Oracle Tuxedo",
        icon: "tuxedo",
        layer: "Middleware",
        process_names: &["tmboot", "tuxipc", "BBL", "DBBL"],
        cmdline_patterns: &["tuxedo", "TUXDIR", "tmboot", "BBL"],
        port_hints: &[],
        windows_commands: Some(TechCommands {
            check: "tmadmin -r <<< 'psr'",
            start: Some("tmboot -y"),
            stop: Some("tmshutdown -y"),
            restart: Some("tmshutdown -y && tmboot -y"),
            logs: Some(r#"type "%APPDIR%\ULOG*" | more"#),
            version: Some("tmadmin -v"),
        }),
        linux_commands: Some(TechCommands {
            check: "tmadmin -r <<< 'psr'",
            start: Some("tmboot -y"),
            stop: Some("tmshutdown -y"),
            restart: Some("tmshutdown -y && tmboot -y"),
            logs: Some("tail -100 $APPDIR/ULOG*"),
            version: Some("tmadmin -v"),
        }),
    },

    // =========================================================================
    // MIDDLEWARE - IBM WEBSPHERE
    // =========================================================================
    TechPattern {
        id: "websphere",
        display_name: "IBM WebSphere",
        icon: "websphere",
        layer: "Application",
        process_names: &[],
        cmdline_patterns: &["com.ibm.ws.runtime.WsServer", "WebSphere", "was.install.root"],
        port_hints: &[9060, 9080, 9043, 9443],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%WsServer%'" | findstr java"#,
            start: Some(r#"{WAS_HOME}\bin\startServer.bat {SERVER}"#),
            stop: Some(r#"{WAS_HOME}\bin\stopServer.bat {SERVER}"#),
            restart: None,
            logs: Some(r#"type "{WAS_HOME}\profiles\{PROFILE}\logs\{SERVER}\SystemOut.log" | more"#),
            version: Some(r#"{WAS_HOME}\bin\versionInfo.bat"#),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep 'WsServer' | grep -v grep",
            start: Some("$WAS_HOME/bin/startServer.sh $SERVER"),
            stop: Some("$WAS_HOME/bin/stopServer.sh $SERVER"),
            restart: None,
            logs: Some("tail -100 $WAS_HOME/profiles/$PROFILE/logs/$SERVER/SystemOut.log"),
            version: Some("$WAS_HOME/bin/versionInfo.sh"),
        }),
    },
    TechPattern {
        id: "liberty",
        display_name: "WebSphere Liberty",
        icon: "websphere",
        layer: "Application",
        process_names: &[],
        cmdline_patterns: &["wlp/lib", "liberty", "ws-server"],
        port_hints: &[9080, 9443],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%wlp%'" | findstr java"#,
            start: Some(r#"{WLP_HOME}\bin\server start {SERVER}"#),
            stop: Some(r#"{WLP_HOME}\bin\server stop {SERVER}"#),
            restart: None,
            logs: Some(r#"type "{WLP_HOME}\usr\servers\{SERVER}\logs\messages.log" | more"#),
            version: Some(r#"{WLP_HOME}\bin\productInfo version"#),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep 'wlp' | grep -v grep",
            start: Some("$WLP_HOME/bin/server start $SERVER"),
            stop: Some("$WLP_HOME/bin/server stop $SERVER"),
            restart: None,
            logs: Some("tail -100 $WLP_HOME/usr/servers/$SERVER/logs/messages.log"),
            version: Some("$WLP_HOME/bin/productInfo version"),
        }),
    },

    // =========================================================================
    // MESSAGE QUEUES
    // =========================================================================
    TechPattern {
        id: "rabbitmq",
        display_name: "RabbitMQ",
        icon: "rabbitmq",
        layer: "Middleware",
        process_names: &["erl", "erl.exe", "beam.smp"],
        cmdline_patterns: &["rabbit", "mnesia"],
        port_hints: &[5672, 15672],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "Caption = 'erl.exe'" get caption | findstr erl.exe"#,
            start: Some(r#"{INSTALL_DIR}\sbin\rabbitmq-service.bat start"#),
            stop: Some(r#"{INSTALL_DIR}\sbin\rabbitmq-service.bat stop"#),
            restart: None,
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "{INSTALL_DIR}\log\*.log""#),
            version: Some("rabbitmq-diagnostics server_version"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active rabbitmq-server",
            start: Some("systemctl start rabbitmq-server"),
            stop: Some("systemctl stop rabbitmq-server"),
            restart: Some("systemctl restart rabbitmq-server"),
            logs: Some("journalctl -u rabbitmq-server -n 100 --no-pager"),
            version: Some("rabbitmqctl version"),
        }),
    },
    TechPattern {
        id: "kafka",
        display_name: "Apache Kafka",
        icon: "kafka",
        layer: "Middleware",
        process_names: &[],
        cmdline_patterns: &["kafka.Kafka", "kafka-server-start"],
        port_hints: &[9092],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%kafka.Kafka%'" | findstr java"#,
            start: None,
            stop: Some(r#"wmic Path win32_process Where "CommandLine Like '%kafka.Kafka%'" Call Terminate"#),
            restart: None,
            logs: Some(r#"type "{INSTALL_DIR}\logs\server.log" | more"#),
            version: None,
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep 'kafka.Kafka' | grep -v grep",
            start: Some("$KAFKA_HOME/bin/kafka-server-start.sh -daemon $KAFKA_HOME/config/server.properties"),
            stop: Some("$KAFKA_HOME/bin/kafka-server-stop.sh"),
            restart: None,
            logs: Some("tail -100 $KAFKA_HOME/logs/server.log"),
            version: None,
        }),
    },
    TechPattern {
        id: "activemq",
        display_name: "ActiveMQ",
        icon: "activemq",
        layer: "Middleware",
        process_names: &[],
        cmdline_patterns: &["activemq", "org.apache.activemq"],
        port_hints: &[61616, 8161],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%activemq%'" | findstr java"#,
            start: Some(r#"{INSTALL_DIR}\bin\activemq start"#),
            stop: Some(r#"{INSTALL_DIR}\bin\activemq stop"#),
            restart: None,
            logs: Some(r#"type "{INSTALL_DIR}\data\activemq.log" | more"#),
            version: Some(r#"{INSTALL_DIR}\bin\activemq --version"#),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep activemq | grep -v grep",
            start: Some("$ACTIVEMQ_HOME/bin/activemq start"),
            stop: Some("$ACTIVEMQ_HOME/bin/activemq stop"),
            restart: Some("$ACTIVEMQ_HOME/bin/activemq restart"),
            logs: Some("tail -100 $ACTIVEMQ_HOME/data/activemq.log"),
            version: Some("$ACTIVEMQ_HOME/bin/activemq --version"),
        }),
    },

    // =========================================================================
    // SCHEDULERS
    // =========================================================================
    TechPattern {
        id: "controlm",
        display_name: "Control-M Agent",
        icon: "controlm",
        layer: "Scheduler",
        process_names: &["ctmag", "p_ctmag", "ag_ping"],
        cmdline_patterns: &["ctm", "controlm", "p_ctmag"],
        port_hints: &[7006, 7005],
        windows_commands: Some(TechCommands {
            check: r#"sc query "Control-M*" | findstr RUNNING"#,
            start: Some(r#"net start "Control-M/Agent""#),
            stop: Some(r#"net stop "Control-M/Agent""#),
            restart: None,
            logs: Some(r#"type "%CONTROLM%\proclog\*.log" | more"#),
            version: Some("ctm -version"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep p_ctmag | grep -v grep",
            start: Some("start-ag -u controlm -p all"),
            stop: Some("shut-ag -u controlm -p all"),
            restart: Some("shut-ag -u controlm && start-ag -u controlm"),
            logs: Some("tail -100 $CONTROLM/proclog/*.log"),
            version: Some("ctm -version"),
        }),
    },
    TechPattern {
        id: "autosys",
        display_name: "AutoSys",
        icon: "autosys",
        layer: "Scheduler",
        process_names: &["cybAgent", "event_demon"],
        cmdline_patterns: &["autosys", "cybAgent", "CA/SharedComponents"],
        port_hints: &[9000],
        windows_commands: Some(TechCommands {
            check: r#"sc query "CA-AutoSys*" | findstr RUNNING"#,
            start: Some(r#"net start "CA Workload Automation Agent""#),
            stop: Some(r#"net stop "CA Workload Automation Agent""#),
            restart: None,
            logs: Some(r#"type "%AUTOUSER%\out\*.out" | more"#),
            version: Some("autoflags -a"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep cybAgent | grep -v grep",
            start: Some("cybAgent start"),
            stop: Some("cybAgent stop"),
            restart: Some("cybAgent restart"),
            logs: Some("tail -100 $AUTOUSER/out/*.out"),
            version: Some("autoflags -a"),
        }),
    },
    TechPattern {
        id: "dollaruniverse",
        display_name: "Dollar Universe",
        icon: "dollaruniverse",
        layer: "Scheduler",
        process_names: &["uvms", "uxsrsw"],
        cmdline_patterns: &["universe", "UNIVDIR", "uvms"],
        port_hints: &[3551, 3552],
        windows_commands: Some(TechCommands {
            check: r#"sc query "Dollar Universe*" | findstr RUNNING"#,
            start: Some(r#"net start "Dollar Universe""#),
            stop: Some(r#"net stop "Dollar Universe""#),
            restart: None,
            logs: Some(r#"type "%UNIVDIR%\log\*.log" | more"#),
            version: None,
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep uvms | grep -v grep",
            start: Some("uxordgx -start"),
            stop: Some("uxordgx -stop"),
            restart: None,
            logs: Some("tail -100 $UNIVDIR/log/*.log"),
            version: None,
        }),
    },
    TechPattern {
        id: "tws",
        display_name: "IBM TWS",
        icon: "tws",
        layer: "Scheduler",
        process_names: &["netman", "mailman", "jobman"],
        cmdline_patterns: &["TWA/", "twsinst", "netman"],
        port_hints: &[31111, 31114],
        windows_commands: Some(TechCommands {
            check: r#"sc query "TWSNTSERVICE*" | findstr RUNNING"#,
            start: Some("conman start"),
            stop: Some("conman stop"),
            restart: None,
            logs: Some(r#"type "%TWS_HOME%\stdlist\*" | more"#),
            version: Some("conman -v"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep netman | grep -v grep",
            start: Some("conman start"),
            stop: Some("conman stop"),
            restart: None,
            logs: Some("tail -100 $TWS_HOME/stdlist/*"),
            version: Some("conman -v"),
        }),
    },

    // =========================================================================
    // FILE TRANSFER
    // =========================================================================
    TechPattern {
        id: "connectdirect",
        display_name: "Connect:Direct",
        icon: "connectdirect",
        layer: "File Transfer",
        process_names: &["ndmcmgr", "cdpmgr", "cdpmgr.exe"],
        cmdline_patterns: &["ndm", "Sterling", "Connect:Direct", "cdpmgr"],
        port_hints: &[1364],
        windows_commands: Some(TechCommands {
            check: r#"sc query "Connect:Direct*" | findstr RUNNING"#,
            start: Some(r#"net start "Connect:Direct""#),
            stop: Some(r#"net stop "Connect:Direct""#),
            restart: None,
            logs: Some(r#"type "%CDDIR%\work\*.log" | more"#),
            version: Some("direct -v"),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep ndmcmgr | grep -v grep",
            start: Some("cdpmgr -s"),
            stop: Some("cdpmgr -t"),
            restart: None,
            logs: Some("tail -100 $CDDIR/work/*.log"),
            version: Some("direct -v"),
        }),
    },
    TechPattern {
        id: "axway",
        display_name: "Axway Transfer CFT",
        icon: "axway",
        layer: "File Transfer",
        process_names: &["cft", "copilot"],
        cmdline_patterns: &["cft", "Transfer_CFT", "Axway"],
        port_hints: &[1761, 1762],
        windows_commands: Some(TechCommands {
            check: r#"cftutil ABOUT"#,
            start: Some("cft start"),
            stop: Some("cft stop"),
            restart: Some("cft stop && cft start"),
            logs: Some(r#"type "%CFTDIRRUNTIME%\log\cft.log" | more"#),
            version: Some("cftutil ABOUT"),
        }),
        linux_commands: Some(TechCommands {
            check: "cftutil ABOUT",
            start: Some("cft start"),
            stop: Some("cft stop"),
            restart: Some("cft stop && cft start"),
            logs: Some("tail -100 $CFTDIRRUNTIME/log/cft.log"),
            version: Some("cftutil ABOUT"),
        }),
    },

    // =========================================================================
    // SECURITY
    // =========================================================================
    TechPattern {
        id: "cyberark",
        display_name: "CyberArk Vault",
        icon: "cyberark",
        layer: "Security",
        process_names: &["PrivateArk", "CyberArk"],
        cmdline_patterns: &["PrivateArk", "CyberArk", "Vault"],
        port_hints: &[1858],
        windows_commands: Some(TechCommands {
            check: r#"sc query "CyberArk*" | findstr RUNNING"#,
            start: Some(r#"net start "CyberArk Vault""#),
            stop: Some(r#"net stop "CyberArk Vault""#),
            restart: None,
            logs: Some(r#"type "C:\Program Files\PrivateArk\Server\Logs\*.log" | more"#),
            version: None,
        }),
        linux_commands: None,
    },

    // =========================================================================
    // WEB SERVERS / PROXIES
    // =========================================================================
    TechPattern {
        id: "nginx",
        display_name: "Nginx",
        icon: "nginx",
        layer: "Access Points",
        process_names: &["nginx", "nginx.exe"],
        cmdline_patterns: &["nginx"],
        port_hints: &[80, 443, 8080, 8443],
        windows_commands: Some(TechCommands {
            check: r#"wmic process where "ExecutablePath like '%nginx.exe'" get ProcessID | findstr /R "[0-9]""#,
            start: Some("nginx"),
            stop: Some(r#"wmic process where "ExecutablePath like '%nginx.exe'" call terminate"#),
            restart: Some("nginx -s reload"),
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "{INSTALL_DIR}\logs\error.log""#),
            version: Some("nginx -v"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active nginx",
            start: Some("systemctl start nginx"),
            stop: Some("systemctl stop nginx"),
            restart: Some("systemctl reload nginx"),
            logs: Some("tail -100 /var/log/nginx/error.log"),
            version: Some("nginx -v"),
        }),
    },
    TechPattern {
        id: "apache",
        display_name: "Apache HTTP",
        icon: "apache",
        layer: "Access Points",
        process_names: &["httpd", "apache2", "httpd.exe"],
        cmdline_patterns: &["httpd", "apache2"],
        port_hints: &[80, 443],
        windows_commands: Some(TechCommands {
            check: r#"sc query Apache* | findstr RUNNING"#,
            start: Some("net start Apache*"),
            stop: Some("net stop Apache*"),
            restart: None,
            logs: Some(r#"type "C:\Apache*\logs\error.log" | more"#),
            version: Some("httpd -v"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active apache2 || systemctl is-active httpd",
            start: Some("systemctl start apache2"),
            stop: Some("systemctl stop apache2"),
            restart: Some("systemctl reload apache2"),
            logs: Some("tail -100 /var/log/apache2/error.log"),
            version: Some("apache2 -v"),
        }),
    },
    TechPattern {
        id: "haproxy",
        display_name: "HAProxy",
        icon: "haproxy",
        layer: "Access Points",
        process_names: &["haproxy"],
        cmdline_patterns: &["haproxy"],
        port_hints: &[],
        windows_commands: None,
        linux_commands: Some(TechCommands {
            check: "systemctl is-active haproxy",
            start: Some("systemctl start haproxy"),
            stop: Some("systemctl stop haproxy"),
            restart: Some("systemctl reload haproxy"),
            logs: Some("journalctl -u haproxy -n 100 --no-pager"),
            version: Some("haproxy -v"),
        }),
    },
    TechPattern {
        id: "iis",
        display_name: "IIS",
        icon: "iis",
        layer: "Access Points",
        process_names: &["w3wp.exe", "iisexpress.exe"],
        cmdline_patterns: &["w3wp", "iisexpress"],
        port_hints: &[80, 443],
        windows_commands: Some(TechCommands {
            check: "sc query W3SVC | findstr RUNNING",
            start: Some("net start W3SVC"),
            stop: Some("net stop W3SVC"),
            restart: Some("iisreset /restart"),
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "C:\inetpub\logs\LogFiles\W3SVC*\*.log""#),
            version: Some(r#"reg query "HKLM\SOFTWARE\Microsoft\InetStp" /v VersionString"#),
        }),
        linux_commands: None,
    },
    TechPattern {
        id: "f5",
        display_name: "F5 BIG-IP",
        icon: "f5",
        layer: "Access Points",
        process_names: &["tmm", "mcpd", "named"],
        cmdline_patterns: &["bigip", "f5"],
        port_hints: &[443],
        windows_commands: None,
        linux_commands: Some(TechCommands {
            check: "tmsh show sys version",
            start: None,
            stop: None,
            restart: None,
            logs: Some("tail -100 /var/log/ltm"),
            version: Some("tmsh show sys version"),
        }),
    },

    // =========================================================================
    // APPLICATION SERVERS
    // =========================================================================
    TechPattern {
        id: "tomcat",
        display_name: "Apache Tomcat",
        icon: "tomcat",
        layer: "Application",
        process_names: &[],
        cmdline_patterns: &["catalina", "org.apache.catalina.startup.Bootstrap"],
        port_hints: &[8080, 8443],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%catalina%' and Caption = 'java.exe'" | findstr java"#,
            start: Some(r#"{INSTALL_DIR}\bin\startup.bat"#),
            stop: Some(r#"{INSTALL_DIR}\bin\shutdown.bat"#),
            restart: None,
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "{INSTALL_DIR}\logs\catalina.out""#),
            version: Some(r#"cd /D {INSTALL_DIR}\bin && catalina version"#),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep catalina | grep -v grep",
            start: Some("$CATALINA_HOME/bin/startup.sh"),
            stop: Some("$CATALINA_HOME/bin/shutdown.sh"),
            restart: None,
            logs: Some("tail -100 $CATALINA_HOME/logs/catalina.out"),
            version: Some("$CATALINA_HOME/bin/catalina.sh version"),
        }),
    },
    TechPattern {
        id: "jboss",
        display_name: "JBoss/WildFly",
        icon: "jboss",
        layer: "Application",
        process_names: &[],
        cmdline_patterns: &["jboss.home.dir", "org.jboss.as.standalone", "wildfly"],
        port_hints: &[8080, 9990],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "CommandLine Like '%jboss%' and Caption = 'java.exe'" | findstr java"#,
            start: Some(r#"{INSTALL_DIR}\bin\standalone.bat"#),
            stop: Some(r#"wmic Path win32_process Where "CommandLine Like '%jboss%'" Call Terminate"#),
            restart: None,
            logs: Some(r#"type "{INSTALL_DIR}\standalone\log\server.log" | more"#),
            version: Some(r#"cd /D {INSTALL_DIR}\bin && standalone.bat --version"#),
        }),
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep jboss | grep -v grep",
            start: Some("$JBOSS_HOME/bin/standalone.sh &"),
            stop: Some("$JBOSS_HOME/bin/jboss-cli.sh --connect --command=shutdown"),
            restart: None,
            logs: Some("tail -100 $JBOSS_HOME/standalone/log/server.log"),
            version: Some("$JBOSS_HOME/bin/standalone.sh --version"),
        }),
    },
    TechPattern {
        id: "nodejs",
        display_name: "Node.js",
        icon: "nodejs",
        layer: "Application",
        process_names: &["node", "node.exe"],
        cmdline_patterns: &["node "],
        port_hints: &[3000, 8080],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "Caption = 'node.exe'" | findstr node"#,
            start: None,
            stop: Some(r#"wmic Path win32_process Where "Caption = 'node.exe'" Call Terminate"#),
            restart: None,
            logs: None,
            version: Some("node --version"),
        }),
        linux_commands: Some(TechCommands {
            check: "pgrep -f 'node '",
            start: None,
            stop: Some("pkill -f 'node '"),
            restart: None,
            logs: None,
            version: Some("node --version"),
        }),
    },
    TechPattern {
        id: "dotnet",
        display_name: ".NET",
        icon: "dotnet",
        layer: "Application",
        process_names: &["dotnet", "dotnet.exe"],
        cmdline_patterns: &["dotnet "],
        port_hints: &[5000, 5001],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "Caption = 'dotnet.exe'" | findstr dotnet"#,
            start: None,
            stop: Some(r#"wmic Path win32_process Where "Caption = 'dotnet.exe'" Call Terminate"#),
            restart: None,
            logs: None,
            version: Some("dotnet --version"),
        }),
        linux_commands: Some(TechCommands {
            check: "pgrep -f dotnet",
            start: None,
            stop: Some("pkill -f dotnet"),
            restart: None,
            logs: None,
            version: Some("dotnet --version"),
        }),
    },

    // =========================================================================
    // XCOMPONENT (Invivoo specific)
    // =========================================================================
    TechPattern {
        id: "xcomponent",
        display_name: "XComponent",
        icon: "xcomponent",
        layer: "Application",
        process_names: &["xcruntime.exe", "xcruntime"],
        cmdline_patterns: &["xcruntime", ".xcproperties", ".xcr"],
        port_hints: &[],
        windows_commands: Some(TechCommands {
            check: r#"wmic Path win32_process Where "Caption = 'xcruntime.exe' and CommandLine Like '%{SERVICE_NAME}.xcproperties%'" | findstr xcruntime"#,
            start: Some(r#"{INSTALL_DIR}\xcruntime.exe {INSTALL_DIR}\{SERVICE_NAME}.xcr {INSTALL_DIR}\{SERVICE_NAME}.xcproperties >NUL 2>&1"#),
            stop: Some(r#"wmic Path win32_process Where "CommandLine Like '%xcruntime%{SERVICE_NAME}.xcr%'" Call Terminate"#),
            restart: None,
            logs: Some(r#"powershell Get-Content -Tail 100 -Path "{INSTALL_DIR}\*.log""#),
            version: None,
        }),
        linux_commands: None,
    },

    // =========================================================================
    // INFRASTRUCTURE
    // =========================================================================
    TechPattern {
        id: "docker",
        display_name: "Docker",
        icon: "docker",
        layer: "Infrastructure",
        process_names: &["dockerd", "docker.exe"],
        cmdline_patterns: &["dockerd"],
        port_hints: &[2375, 2376],
        windows_commands: Some(TechCommands {
            check: "docker info >NUL 2>&1 && echo RUNNING",
            start: Some("net start docker"),
            stop: Some("net stop docker"),
            restart: None,
            logs: None,
            version: Some("docker --version"),
        }),
        linux_commands: Some(TechCommands {
            check: "systemctl is-active docker",
            start: Some("systemctl start docker"),
            stop: Some("systemctl stop docker"),
            restart: Some("systemctl restart docker"),
            logs: Some("journalctl -u docker -n 100 --no-pager"),
            version: Some("docker --version"),
        }),
    },
    TechPattern {
        id: "zookeeper",
        display_name: "ZooKeeper",
        icon: "zookeeper",
        layer: "Infrastructure",
        process_names: &[],
        cmdline_patterns: &["zookeeper", "QuorumPeerMain"],
        port_hints: &[2181],
        windows_commands: None,
        linux_commands: Some(TechCommands {
            check: "ps -ef | grep QuorumPeerMain | grep -v grep",
            start: Some("$ZOOKEEPER_HOME/bin/zkServer.sh start"),
            stop: Some("$ZOOKEEPER_HOME/bin/zkServer.sh stop"),
            restart: Some("$ZOOKEEPER_HOME/bin/zkServer.sh restart"),
            logs: Some("tail -100 $ZOOKEEPER_HOME/logs/zookeeper*.log"),
            version: Some("$ZOOKEEPER_HOME/bin/zkServer.sh version"),
        }),
    },
];

/// Match a process against known technology patterns.
pub fn identify_technology(
    process_name: &str,
    cmdline: &str,
    ports: &[u16],
) -> Option<TechnologyHint> {
    let name_lower = process_name.to_lowercase();
    let cmdline_lower = cmdline.to_lowercase();

    for pattern in TECH_PATTERNS {
        let name_match = pattern
            .process_names
            .iter()
            .any(|p| name_lower == *p || name_lower == format!("{}.exe", p));

        let cmdline_match = pattern
            .cmdline_patterns
            .iter()
            .any(|p| cmdline_lower.contains(&p.to_lowercase()));

        let port_match = !pattern.port_hints.is_empty()
            && pattern.port_hints.iter().any(|p| ports.contains(p));

        // Require at least one strong signal (name or cmdline)
        if name_match || cmdline_match {
            return Some(TechnologyHint {
                id: pattern.id.to_string(),
                display_name: pattern.display_name.to_string(),
                icon: pattern.icon.to_string(),
                layer: pattern.layer.to_string(),
            });
        }

        // Port-only match is too weak, skip
        if port_match && !name_match && !cmdline_match {
            continue;
        }
    }

    None
}

/// Get commands for a technology by ID.
#[allow(dead_code)]
pub fn get_commands_by_id(tech_id: &str) -> Option<CommandSuggestion> {
    let pattern = TECH_PATTERNS.iter().find(|p| p.id == tech_id)?;

    #[cfg(target_os = "windows")]
    let commands = pattern.windows_commands.as_ref()?;

    #[cfg(not(target_os = "windows"))]
    let commands = pattern.linux_commands.as_ref()?;

    Some(CommandSuggestion {
        check_cmd: commands.check.to_string(),
        start_cmd: commands.start.map(|s| s.to_string()),
        stop_cmd: commands.stop.map(|s| s.to_string()),
        restart_cmd: commands.restart.map(|s| s.to_string()),
        logs_cmd: commands.logs.map(|s| s.to_string()),
        version_cmd: commands.version.map(|s| s.to_string()),
        confidence: "high".to_string(),
        source: tech_id.to_string(),
    })
}

/// Identify technology and get commands from process name, cmdline, and ports.
///
/// Returns (technology_display_name, commands) if a match is found.
/// This is the main entry point for Windows discovery to identify Java apps
/// like Elasticsearch, Tomcat, Kafka, etc. based on their command line.
pub fn get_commands_for_technology(
    process_name: &str,
    cmdline: &str,
    ports: &[u16],
) -> Option<(String, CommandSuggestion)> {
    let name_lower = process_name.to_lowercase();
    let cmdline_lower = cmdline.to_lowercase();

    for pattern in TECH_PATTERNS {
        let name_match = pattern
            .process_names
            .iter()
            .any(|p| name_lower == *p || name_lower == format!("{}.exe", p));

        let cmdline_match = pattern
            .cmdline_patterns
            .iter()
            .any(|p| cmdline_lower.contains(&p.to_lowercase()));

        let port_match = !pattern.port_hints.is_empty()
            && pattern.port_hints.iter().any(|p| ports.contains(p));

        // Require cmdline match for Java processes (name "java" is too generic)
        // For other processes, name match is sufficient
        let is_java = name_lower == "java" || name_lower == "java.exe" || name_lower.contains("javaw");
        let has_match = if is_java {
            cmdline_match // Java requires cmdline match
        } else {
            name_match || cmdline_match
        };

        if has_match || (port_match && (name_match || cmdline_match)) {
            // Get platform-specific commands
            #[cfg(target_os = "windows")]
            let commands_opt = pattern.windows_commands.as_ref();

            #[cfg(not(target_os = "windows"))]
            let commands_opt = pattern.linux_commands.as_ref();

            if let Some(commands) = commands_opt {
                return Some((
                    pattern.display_name.to_string(),
                    CommandSuggestion {
                        check_cmd: commands.check.to_string(),
                        start_cmd: commands.start.map(|s| s.to_string()),
                        stop_cmd: commands.stop.map(|s| s.to_string()),
                        restart_cmd: commands.restart.map(|s| s.to_string()),
                        logs_cmd: commands.logs.map(|s| s.to_string()),
                        version_cmd: commands.version.map(|s| s.to_string()),
                        confidence: if cmdline_match { "high" } else { "medium" }.to_string(),
                        source: pattern.id.to_string(),
                    },
                ));
            }
        }
    }

    None
}

/// Extract service name from XComponent cmdline.
#[allow(dead_code)]
pub fn extract_xcomponent_service_name(cmdline: &str) -> Option<String> {
    if let Some(idx) = cmdline.find(".xcproperties") {
        let before = &cmdline[..idx];
        let start = before
            .rfind(|c: char| c == '\\' || c == '/' || c == ' ')
            .map(|i| i + 1)
            .unwrap_or(0);
        let name = &before[start..];
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_elasticsearch() {
        let hint = identify_technology(
            "java.exe",
            "org.elasticsearch.bootstrap.Elasticsearch",
            &[9200],
        );
        assert!(hint.is_some());
        assert_eq!(hint.unwrap().id, "elasticsearch");
    }

    #[test]
    fn test_identify_ibmmq() {
        let hint = identify_technology("amqzxma0", "amqzxma0 QMGR(QM1)", &[1414]);
        assert!(hint.is_some());
        assert_eq!(hint.unwrap().id, "ibmmq");
    }

    #[test]
    fn test_identify_weblogic() {
        let hint = identify_technology(
            "java.exe",
            "weblogic.Server -Dweblogic.Name=AdminServer",
            &[7001],
        );
        assert!(hint.is_some());
        assert_eq!(hint.unwrap().id, "weblogic");
    }

    #[test]
    fn test_identify_controlm() {
        let hint = identify_technology("p_ctmag", "/opt/ctm/ctm/exe/p_ctmag", &[7006]);
        assert!(hint.is_some());
        assert_eq!(hint.unwrap().id, "controlm");
    }

    #[test]
    fn test_identify_oracle() {
        let hint = identify_technology("ora_pmon_ORCL", "ora_pmon_ORCL", &[1521]);
        assert!(hint.is_some());
        assert_eq!(hint.unwrap().id, "oracle");
    }

    #[test]
    fn test_identify_connectdirect() {
        let hint = identify_technology("ndmcmgr", "ndmcmgr", &[1364]);
        assert!(hint.is_some());
        assert_eq!(hint.unwrap().id, "connectdirect");
    }

    #[test]
    fn test_no_false_positive() {
        let hint = identify_technology("myapp.exe", "myapp --config app.ini", &[9999]);
        assert!(hint.is_none());
    }
}
