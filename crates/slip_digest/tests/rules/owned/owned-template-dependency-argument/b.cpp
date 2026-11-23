#include "dep.h"
namespace acme { template <class T> struct Holder { T t; }; }
int use() { acme::Holder<legacy::Dep> h; return sizeof h; }
