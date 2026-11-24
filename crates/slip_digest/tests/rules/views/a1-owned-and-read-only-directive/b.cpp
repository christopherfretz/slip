#include "new_dep.h"
namespace acme { using namespace std; }
template <class T> T g(T t) { return acme::forward<T>(t); }
