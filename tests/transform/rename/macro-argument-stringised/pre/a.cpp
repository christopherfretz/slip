int check(int v, const char *text);
#define CHECK(x) check((x), #x)
namespace a { int f(int x) { return x; } }
int u() { return CHECK(a::f(8)); }
