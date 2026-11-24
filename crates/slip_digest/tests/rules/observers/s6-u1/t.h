#ifdef POST
namespace fresh { inline int twice(int x) { return 2 * x; } }
#else
namespace old { inline int twice(int x) { return 2 * x; } }
#endif
