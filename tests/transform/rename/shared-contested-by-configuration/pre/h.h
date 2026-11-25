#pragma once
namespace a { int f(int); }
namespace b { int f(int); }
#ifdef ALT
#define NS b
#else
#define NS a
#endif
inline int call() {
  using namespace NS;
  return f(1);
}
