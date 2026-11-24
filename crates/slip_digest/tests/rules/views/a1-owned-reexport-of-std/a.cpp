#include "std.h"
#include "new_dep.h"
namespace legacy { using std::string; using std::forward; }
using legacy::string;
template <class T> T g(T t) { return legacy::forward<T>(t); }
