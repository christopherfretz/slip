namespace a { long f(int x) { return x; } }
decltype(a::f(1)) v = 0;
using P = decltype(&a::f);
int s = sizeof(a::f(1));
bool n = noexcept(a::f(1));
void h() noexcept(noexcept(a::f(2))) {}
int al = alignof(decltype(a::f(3)));
