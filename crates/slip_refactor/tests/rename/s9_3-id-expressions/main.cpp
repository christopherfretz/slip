namespace n {
int f(int);
struct C { static int f(int); };
}
template <int (*P)(int)> struct T {};
auto p = &n::f;
auto q = &n::C::f;
using S = decltype(n::f(1));
static_assert(sizeof(n::f(1)) == sizeof(int));
void d(int a = n::f(1));
struct M { int m = n::f(2); };
T<n::f> t;
