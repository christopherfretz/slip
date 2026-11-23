namespace m { namespace n { extern "C" void f(int); } }
namespace n { void f(long); }
void h() { m::n::f(1); n::f(2L); }
