_uninstall_complete() {
    local current
    current=${COMP_WORDS[COMP_CWORD]}
    COMPREPLY=($(compgen -W '--show-dependencies --json --debug --backend --confirm --self-uninstall --version --help' -- "$current"))
}
complete -F _uninstall_complete uninstall
