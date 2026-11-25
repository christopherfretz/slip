namespace a { constexpr int g(int x) { return x; } }
auto l = [](int y) { return a::g(y); };
template <class T> concept C = requires(T t) { a::g(t); };
static_assert(C<int>);
static_assert(a::g(3) == 3);
int u() { return l(1); }
