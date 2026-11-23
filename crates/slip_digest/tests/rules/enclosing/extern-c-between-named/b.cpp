namespace n { }
namespace m { extern "C" int cf() { return 1; } }
int u() { return m::cf(); }
