namespace m { void f(int); }
namespace n { void f(long); }
using namespace m;
using namespace n;
template <class T> void t(T x) { f(x); }
