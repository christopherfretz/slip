#include "std.h"
#include "new_dep.h"
namespace legacy { using std::string; using std::forward; template <class T> T forward(T t, int); }
using legacy::string;
template <class T> T g(T t) { return legacy::forward<T>(t); }
