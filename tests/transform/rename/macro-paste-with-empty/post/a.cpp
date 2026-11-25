#define CAT(p, q) p##q
namespace a {
int g(int x) { return x; }
int u() { return CAT(g, )(1) + CAT(, g)(2); }
}
