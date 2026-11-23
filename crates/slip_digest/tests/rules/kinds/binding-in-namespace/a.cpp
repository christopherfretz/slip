// std: c++20
namespace acme {
struct P { int a; int b; };
P p{1, 2};
auto [x, y] = p;
}
int use() { return acme::x + acme::y; }
