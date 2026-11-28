// errors: yes
#include "side.h"
// WP-136 ("Tests"): a specialisation invalid because of a real error
// (`DeclFlags::INVALID` without Kiln's `subst_failed` mark) stays a unit and
// stays indeterminate (`Cause::Error`). Kills the mutation that drops every
// invalid specialisation, error or not.
template <class T> typename T::type v = 0;
int x = v<int>;
