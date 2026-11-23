namespace n { int v; }
namespace m { using n::v; }
int h() { return m::v; }
