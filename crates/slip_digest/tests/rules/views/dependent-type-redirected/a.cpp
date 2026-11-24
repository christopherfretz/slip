#include "dep.h"
namespace legacy { template <class T> struct X { using type = T; }; }
template <class T> typename legacy::X<T>::type f(T t) { return t; }
