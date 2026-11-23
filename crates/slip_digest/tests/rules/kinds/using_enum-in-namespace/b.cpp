// std: c++20
namespace legacy {
enum class E { A, B };
struct S { using enum E; E v = B; };
}
int use() { return (int)legacy::S().v; }
