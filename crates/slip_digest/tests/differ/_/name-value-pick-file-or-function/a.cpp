// file name or function name, chosen from a line
consteval const char *pick(unsigned l = __builtin_LINE(), const char *f = __builtin_FILE(), const char *g = __builtin_FUNCTION()) { return l % 2 ? f : g; }
void take(const char *);
void h() { take(pick()); }
