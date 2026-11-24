#include "dep.h"
namespace legacy { int h(int); int h(long); }
template <class T> auto f(T t) -> decltype(legacy::h(t));
