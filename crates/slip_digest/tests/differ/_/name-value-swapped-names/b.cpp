// WP-104 (digest §9), F1: two functions' __func__ strings with the names swapped and the rule swapping them back; side B calls the other function, whose string has side A's bytes
consteval const char *n2() { return __func__; }
consteval const char *n1() { return __func__; }
void take(const char *);
void f() { take(n1()); }
void g() { take(n2()); }
