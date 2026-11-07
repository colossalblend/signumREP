package pw.appbank.signum.data.apple

import org.junit.Assert.assertTrue
import org.junit.Test

class IpaMinIosTest {
    @Test
    fun readsAppPlistInsteadOfFrameworkPlist() {
        assertTrue(IpaMinIos.selfCheck())
    }
}
