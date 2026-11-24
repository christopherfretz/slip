namespace a { constexpr int f(int x) { return x; } }
auto l = [](int y) { return a::f(y); };
template <class T> concept C = requires(T t) { a::f(t); };
static_assert(C<int>);
static_assert(a::f(3) == 3);
int u() { return l(1); }
