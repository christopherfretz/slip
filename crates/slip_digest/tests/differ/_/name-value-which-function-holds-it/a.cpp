// WP-104 (digest §9), two functions that differ only in which holds the builtin
const char *f() { return __func__; }
const char *g() { return ""; }
