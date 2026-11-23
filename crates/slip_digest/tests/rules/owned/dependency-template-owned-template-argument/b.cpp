#include "dep.h"
namespace acme { template <class T> struct Wrap { T x; }; }
int use() { legacy::apply<acme::Wrap> a; return sizeof a; }
