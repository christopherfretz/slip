// pointer identity chosen at compile time from a line
consteval const char *n1() { return __func__; }
consteval const char *n2() { return __func__; }
consteval const char *pick(unsigned l = __builtin_LINE()) { return l % 2 ? n1() : n2(); }
void take(const char *);
void h() { take(pick()); }
