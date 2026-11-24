#include "std.h"
#include "new_dep.h"
namespace legacy { using namespace std; }
using legacy::string;
template <class T> T g(T t) { return legacy::forward<T>(t); }
