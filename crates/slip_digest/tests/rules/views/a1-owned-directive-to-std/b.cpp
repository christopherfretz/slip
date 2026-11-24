#include "std.h"
#include "new_dep.h"
namespace acme { using namespace std; }
using acme::string;
template <class T> T g(T t) { return acme::forward<T>(t); }
