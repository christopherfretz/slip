#define CAT(p, q) p##q
namespace a {
int f(int x) { return x; }
int u() { return CAT(f, )(1) + CAT(, f)(2); }
}
