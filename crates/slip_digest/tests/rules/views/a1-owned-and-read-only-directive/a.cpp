#include "new_dep.h"
namespace legacy { using namespace std; }
template <class T> T g(T t) { return legacy::forward<T>(t); }
