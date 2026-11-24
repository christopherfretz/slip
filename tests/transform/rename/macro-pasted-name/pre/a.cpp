#define CAT(p, q) p##q
namespace a {
int foo(int x) { return x; }
int u() { return CAT(fo, o)(1); }
}
