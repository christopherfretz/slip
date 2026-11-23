int *mk() { return new int(1); }
namespace __attribute__((visibility("hidden"))) std { enum class align_val_t : decltype(sizeof 0); }
void *al(decltype(sizeof 0) n) { return ::operator new(n, std::align_val_t(16)); }
