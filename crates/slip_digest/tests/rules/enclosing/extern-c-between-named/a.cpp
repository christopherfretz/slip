namespace n { extern "C" int cf() { return 1; } }
namespace m { }
int u() { return n::cf(); }
