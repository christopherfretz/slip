#include "dep.h"
namespace acme { int h(int); }
template <class T> auto f(T t) -> decltype(acme::h(t));
