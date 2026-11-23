// pointer identity chosen at compile time from a name
consteval const char *n1() { return __func__; }
consteval const char *n2() { return __func__; }
consteval const char *pick(const char *p) { return p[0] == 'f' ? n1() : n2(); }
void take(const char *);
void start(int) { take(pick(__func__)); }
