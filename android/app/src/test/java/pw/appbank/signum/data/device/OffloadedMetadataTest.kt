package pw.appbank.signum.data.device

import com.dd.plist.BinaryPropertyListWriter
import com.dd.plist.NSNumber
import com.dd.plist.NSString
import org.junit.Assert.*
import org.junit.Test
import pw.appbank.signum.data.apple.Plist

class OffloadedMetadataTest {
    @Test fun placeholderSupportsBooleanAndNumericPlistValues() {
        assertTrue(placeholderFlag(NSNumber(true)))
        assertTrue(placeholderFlag(NSNumber(1)))
        assertTrue(placeholderFlag(NSString("true")))
        assertFalse(placeholderFlag(NSNumber(false)))
        assertFalse(placeholderFlag(NSNumber(0)))
        assertFalse(placeholderFlag(null))
    }

    @Test fun extractsItemIdFromBinaryAndXmlMetadata() {
        val metadata = Plist.dict("itemId" to 123456789L, "artistId" to 999L)
        assertEquals("123456789", itemIdFromMetadata(BinaryPropertyListWriter.writeToArray(metadata)))
        assertEquals("123456789", itemIdFromMetadata(Plist.xml(metadata)))
        assertNull(itemIdFromMetadata(BinaryPropertyListWriter.writeToArray(Plist.dict("artistId" to 999L))))
        assertNull(itemIdFromMetadata(Plist.xml(Plist.dict("itemId" to ""))))
    }
}
