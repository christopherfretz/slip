#include "new_dep.h"
namespace acme { struct Q {}; }
template <class T> void f(T p) { p->acme::Q::~Q(); }
