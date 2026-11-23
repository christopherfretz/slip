#define g(x) 42
namespace n { int f(int); }
int h() { return n::f(1); }
