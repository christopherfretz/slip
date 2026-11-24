#include "new_dep.h"
namespace legacy { struct Q {}; }
template <class T> void f(T p) { p->legacy::Q::~Q(); }
