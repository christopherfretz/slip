namespace a {
int f(int x) { return x; }
struct C { int m(int) { return 0; } };
}
int (*p)(int) = &a::f;
namespace a {
int (*q)(int) = &f;
}
bool same() { return p == a::q; }
