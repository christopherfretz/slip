namespace m { void f(int); }
namespace n { void f(long); }
using namespace m;
template <class T> void t(T x) { f(x); }
void h() { n::f(1L); }
