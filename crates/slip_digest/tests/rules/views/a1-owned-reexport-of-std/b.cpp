#include "std.h"
#include "new_dep.h"
namespace acme { using std::string; using std::forward; }
using acme::string;
template <class T> T g(T t) { return acme::forward<T>(t); }
