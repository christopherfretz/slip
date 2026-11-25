// f is the function; call f(1).
namespace a { int f(int x) { return x; } }
/* see a::f */
const char *s = "f";
const char *t = "a::f(1)";
const char *n = "of";
int u() { return a::f(1); }
