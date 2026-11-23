#include "dep.h"
namespace legacy { template <class T> struct Wrap { T x; }; }
int use() { legacy::apply<legacy::Wrap> a; return sizeof a; }
