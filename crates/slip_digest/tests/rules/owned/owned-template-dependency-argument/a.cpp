#include "dep.h"
namespace legacy { template <class T> struct Holder { T t; }; }
int use() { legacy::Holder<legacy::Dep> h; return sizeof h; }
