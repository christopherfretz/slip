namespace a {
int f(int);
int g(int);
}
namespace b {
int f(int);
}
#define BOTH(name) \
  namespace a {    \
  int x = name(1); \
  }                \
  namespace b {    \
  int y = name(2); \
  }
BOTH(f)
