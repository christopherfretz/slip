namespace n { void k(); }
namespace m { extern "C" void f(int); }
void h() { m::f(1); }
