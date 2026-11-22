#include "side.h"
// §5.4 FnSpec `0-escalated`: an immediate-escalating specialisation that calls a consteval function becomes immediate.
#ifdef B
consteval int id(int x) { return x; }
constexpr int k(int x) { return x; }
template <class T> constexpr int g(T t) { return id(t); }
int v = g(1);
#else
consteval int id(int x) { return x; }
constexpr int k(int x) { return x; }
template <class T> constexpr int g(T t) { return k(t); }
int v = g(1);
#endif
