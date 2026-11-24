#include "dep.h"
namespace acme { int h(int); int h(long); }
template <class T> auto f(T t) -> decltype(acme::h(t));
