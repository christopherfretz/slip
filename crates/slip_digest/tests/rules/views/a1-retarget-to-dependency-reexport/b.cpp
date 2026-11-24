#include "new_dep.h"
namespace acme { int keep(); }
using acme::string;
template <class T> T g(T t) { return acme::forward<T>(t); }
