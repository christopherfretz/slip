namespace n { void f(int); namespace inner { int g; } }
namespace m { int g; }
void h() { int g = 0; n::f(g); }
