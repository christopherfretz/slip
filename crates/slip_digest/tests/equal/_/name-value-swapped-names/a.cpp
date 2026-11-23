// WP-104 (digest §9), F1: two functions' __func__ strings, the names swapped under a rule that swaps them back
consteval const char *n1() { return __func__; }
consteval const char *n2() { return __func__; }
void take(const char *);
void f() { take(n1()); }
void g() { take(n2()); }
