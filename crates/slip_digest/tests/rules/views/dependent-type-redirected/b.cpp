#include "dep.h"
namespace acme { template <class T> struct X { using type = T; }; }
template <class T> typename acme::X<T>::type f(T t) { return t; }
