namespace a { const char *f() { return __func__; } }
const char *u() { return a::f(); }
