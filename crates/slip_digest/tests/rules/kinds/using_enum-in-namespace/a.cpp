// std: c++20
namespace acme {
enum class E { A, B };
struct S { using enum E; E v = B; };
}
int use() { return (int)acme::S().v; }
