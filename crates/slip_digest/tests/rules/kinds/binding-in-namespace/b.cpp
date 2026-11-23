// std: c++20
namespace legacy {
struct P { int a; int b; };
P p{1, 2};
auto [x, y] = p;
}
int use() { return legacy::x + legacy::y; }
