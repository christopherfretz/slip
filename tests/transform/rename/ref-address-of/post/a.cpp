namespace a {
int g(int x) { return x; }
struct C { int m(int) { return 0; } };
}
int (*p)(int) = &a::g;
namespace a {
int (*q)(int) = &g;
}
bool same() { return p == a::q; }
