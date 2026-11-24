namespace a { long g(int x) { return x; } }
decltype(a::g(1)) v = 0;
using P = decltype(&a::g);
int s = sizeof(a::g(1));
bool n = noexcept(a::g(1));
void h() noexcept(noexcept(a::g(2))) {}
int al = alignof(decltype(a::g(3)));
