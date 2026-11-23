// WP-104 (digest §9), two functions that differ only in which holds the builtin
const char *f() { return ""; }
const char *g() { return __func__; }
