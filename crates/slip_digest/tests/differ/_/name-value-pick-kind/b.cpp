// WP-104 (digest §9), F1: which builtin's string a compile-time choice returns
consteval const char *n1() { return __func__; }
consteval const char *n2() { return __FUNCTION__; }
consteval const char *pick(const char *p) { return p[0] == 'f' ? n1() : n2(); }
void take(const char *);
void start(int) { take(pick(__func__)); }
