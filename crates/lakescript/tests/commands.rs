//! `command` and `subcommand` *(phase 7, see `ideas/language/cli.md`)*:
//! reading arguments into objects, the help, and what the checker rejects.

use lakescript::{CapturedOutput, Engine};

fn run(source: &str) -> String {
    let output = CapturedOutput::new();
    let mut engine = Engine::with_output(output.clone());
    if let Err(e) = engine.run(source) {
        panic!("{}\n--- script:\n{source}", e.render("test.lks"));
    }
    output.text()
}

fn rejected(source: &str) -> String {
    let mut engine = Engine::with_output(CapturedOutput::new());
    match engine.run(source) {
        Ok(_) => panic!("expected an error:\n{source}"),
        Err(e) => e.message,
    }
}

const BACKUP: &str = "
enum Mode { FAST, SAFE; }

/// Copies a folder to the backup disk.
command Backup {
    /// The folder to copy
    string @source;
    /// Where to copy it
    ?string @dest;
    /// How many old backups to keep
    flag(k) number @keep = 5;
    /// Only show what would be copied
    flag(n) bool @dryRun;
    /// How careful to be
    flag(m) Mode @mode = Mode::SAFE;
    /// Files to leave out
    flag ...string @exclude;
    flag(v, count) number @verbose;
}
";

#[test]
fn flags_and_positional_arguments() {
    let script = format!(
        "{BACKUP}
        const $b = Backup::parse(['src', 'out', '-k3', '--dry-run', '--mode=fast', '--exclude', 'a', '--exclude=b', '-vvv']);
        outln([$b.source, $b.dest, $b.keep, $b.dryRun, $b.mode, $b.exclude, $b.verbose]);
        const $d = Backup::parse(['src']);
        outln([$d.dest, $d.keep, $d.dryRun, $d.mode, $d.exclude, $d.verbose]);
        const $e = Backup::parse(['-nk', '7', '--', '-odd']);
        outln([$e.source, $e.keep, $e.dryRun]);
        outln(Backup::parse(['x', '--dry-run', '--no-dry-run']).dryRun);"
    );
    assert_eq!(
        run(&script),
        "['src', 'out', 3, true, Mode::FAST, ['a', 'b'], 3]\n[null, 5, false, Mode::SAFE, [], 0]\n['-odd', 7, true]\nfalse\n"
    );
}

#[test]
fn arguments_that_dont_match_throw_a_usage_error() {
    let script = format!(
        "{BACKUP}
        for ($args in [[], ['x', '--keep', 'lots'], ['x', '--nope'], ['x', '-m', 'slow'], ['x', 'y', 'z']]) {{
            try {{
                Backup::parse($args);
            }} catch (ValueError $e) {{
                outln($e.message().split('\\n')[0]);
            }}
        }}"
    );
    assert_eq!(
        run(&script),
        "`<source>` is missing\n`--keep` needs a number, not `lots`\nunknown option `--nope`\n`--mode` must be one of fast, safe, not `slow`\nunexpected argument `z`\n"
    );
}

#[test]
fn the_help_comes_from_the_comments() {
    let help = run(&format!("{BACKUP}outln(Backup::help());"));
    assert!(help.starts_with("Copies a folder to the backup disk.\n\nUsage: script [options] <source> [dest]\n"), "{help}");
    assert!(help.contains("  <source>  The folder to copy\n"), "{help}");
    assert!(help.contains("-k, --keep <number>"), "{help}");
    assert!(help.contains("How many old backups to keep (default: 5)"), "{help}");
    assert!(help.contains("How careful to be (fast, safe) (default: safe)"), "{help}");
    assert!(help.contains("-h, --help"), "{help}");
}

const GIT: &str = "
/// A tiny git
command Git {
    /// More output
    flag(v, count) number @verbose;

    /// Records changes
    subcommand Commit {
        flag(m) string @message;
        flag(a) bool @all;
    }

    /// Pushes to a remote
    subcommand Push {
        string @remote;
        ?string @branch;
    }
}
";

#[test]
fn subcommands_have_their_own_fields() {
    let script = format!(
        "{GIT}
        for ($args in [['-v', 'commit', '-am', 'Fix it'], ['push', 'origin']]) {{
            const $git = Git::parse($args);
            outln(switch ($git.subcommand) {{
                case Git::Commit -> f\"commit {{$git.subcommand.message}} {{$git.subcommand.all}} {{$git.verbose}}\";
                case Git::Push -> f\"push {{$git.subcommand.remote}} {{$git.subcommand.branch}}\";
            }});
        }}
        outln(Git::parse(['push', 'o']).subcommand.is(Git::Push));"
    );
    assert_eq!(run(&script), "commit Fix it true 1\npush origin null\ntrue\n");
    let script = format!("{GIT}try {{ Git::parse(['pull']); }} catch (UsageError $e) {{ outln($e.message().split('\\n')[0]); }}");
    assert_eq!(run(&script), "unknown command `pull`: it can be commit, push\n");
}

#[test]
fn the_checker_finds_mistakes_with_commands() {
    assert!(rejected("command A { flag(h) bool @x; }").contains("`-h` is for `--help`"));
    assert!(rejected("command A { flag(x) bool @a; flag(x) bool @b; }").contains("use the short flag `-x`"));
    assert!(rejected("command A { list<string> @a; }").contains("strings, numbers, bools or enums"));
    assert!(rejected("command A { flag(v, count) string @v; }").contains("must be a number"));
    assert!(rejected("command A { ?string @a; string @b; }").contains("goes before `a`, which is optional"));
    assert!(rejected("command A { ...string @a; string @b; }").contains("must be the last one without `flag`"));
    assert!(rejected("command A { string $a; }").contains("written with `@`"));
    assert!(rejected("command A { string @a; void m() {} }").contains("not methods"));
    assert!(rejected("command A { string @a; }\nA('x');").contains("comes from `A::parse()`"));
    assert!(rejected("command A { string @a; }\nnumber $n = A::parse([]).a;").contains("its value is a string"));
    assert!(rejected(&format!("{GIT}outln(switch (Git::parse([]).subcommand) {{ case Git::Push -> 1; }});")).contains("no case for `Git::Commit`"));
    assert!(rejected(&format!("{GIT}outln(Git::parse([]).subcommand.message);")).contains("has no field named `message`"));
}
