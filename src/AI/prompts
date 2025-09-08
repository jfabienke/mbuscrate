I would like you to analyse two C files and familiarise yourself with the implemented protocol.


This is the first file called: mbus-protocol:

//------------------------------------------------------------------------------
// Copyright (C) 2010-2011, Robert Johansson, Raditex AB
// All rights reserved.
//
// rSCADA
// http://www.rSCADA.se
// info@rscada.se
//
//------------------------------------------------------------------------------
#include <assert.h>
#include <ctype.h>
#include <stdio.h>
#include <string.h>
#include "mbus-protocol.h"
static int parse_debug = 0, debug = 0;
static char error_str[512];
#define NITEMS(x) (sizeof(x)/sizeof(x
0
))
//------------------------------------------------------------------------------
// Returns the manufacturer ID according to the manufacturer's 3 byte ASCII code
// or zero when there was an error.
//------------------------------------------------------------------------------
unsigned int
mbus_manufacturer_id(char *manufacturer)
{
unsigned int id;
/*
 * manufacturer must consist of at least 3 alphabetic characters,
 * additional chars are silently ignored.
 */

if (!manufacturer || strlen(manufacturer) < 3)
    return 0;

if (!isalpha(manufacturer[0]) ||
    !isalpha(manufacturer[1]) ||
    !isalpha(manufacturer[2]))
    return 0;

id = (toupper(manufacturer[0]) - 64) * 32 * 32 +
     (toupper(manufacturer[1]) - 64) * 32 +
     (toupper(manufacturer[2]) - 64);

/*
 * Valid input data should be in the range of 'AAA' to 'ZZZ' according to
 * the FLAG Association (http://www.dlms.com/flag/), thus resulting in
 * an ID from 0x0421 to 0x6b5a. If the conversion results in anything not
 * in this range, simply discard it and return 0 instead.
 */
return (0x0421 <= id && id <= 0x6b5a) ? id : 0;

}
//------------------------------------------------------------------------------
// internal data
//------------------------------------------------------------------------------
static mbus_slave_data slave_data[MBUS_MAX_PRIMARY_SLAVES];
//
// trace callbacks
//
void
mbus_dump_recv_event(unsigned char src_type, const char *buff, size_t len)
{
mbus_hex_dump("RECV", buff, len);
}
void
mbus_dump_send_event(unsigned char src_type, const char *buff, size_t len)
{
mbus_hex_dump("SEND", buff, len);
}
//------------------------------------------------------------------------------
/// Return a string that contains an the latest error message.
//------------------------------------------------------------------------------
char *
mbus_error_str()
{
return error_str;
}
void
mbus_error_str_set(char *message)
{
if (message)
{
snprintf(error_str, sizeof(error_str), "%s", message);
}
}
void
mbus_error_reset()
{
snprintf(error_str, sizeof(error_str), "no errors");
}
//------------------------------------------------------------------------------
/// Return a pointer to the slave_data register. This register can be used for
/// storing current slave status.
//------------------------------------------------------------------------------
mbus_slave_data *
mbus_slave_data_get(size_t i)
{
if (i < MBUS_MAX_PRIMARY_SLAVES)
{
return &slave_data[i];
}
return NULL;

}
//------------------------------------------------------------------------------
//
// M-Bus FRAME RELATED FUNCTIONS
//
//------------------------------------------------------------------------------
//------------------------------------------------------------------------------
/// Allocate an M-bus frame data structure and initialize it according to which
/// frame type is requested.
//------------------------------------------------------------------------------
mbus_frame *
mbus_frame_new(int frame_type)
{
mbus_frame *frame = NULL;
if ((frame = malloc(sizeof(mbus_frame))) != NULL)
{
    memset((void *)frame, 0, sizeof(mbus_frame));

    frame->type = frame_type;
    switch (frame->type)
    {
        case MBUS_FRAME_TYPE_ACK:

            frame->start1 = MBUS_FRAME_ACK_START;

            break;

        case MBUS_FRAME_TYPE_SHORT:

            frame->start1 = MBUS_FRAME_SHORT_START;
            frame->stop   = MBUS_FRAME_STOP;

            break;

        case MBUS_FRAME_TYPE_CONTROL:

            frame->start1 = MBUS_FRAME_CONTROL_START;
            frame->start2 = MBUS_FRAME_CONTROL_START;
            frame->length1 = 3;
            frame->length2 = 3;
            frame->stop   = MBUS_FRAME_STOP;

            break;

        case MBUS_FRAME_TYPE_LONG:

            frame->start1 = MBUS_FRAME_LONG_START;
            frame->start2 = MBUS_FRAME_LONG_START;
            frame->stop   = MBUS_FRAME_STOP;

            break;
    }
}

return frame;

}
//------------------------------------------------------------------------------
/// Free the memory resources allocated for the M-Bus frame data structure.
//------------------------------------------------------------------------------
int
mbus_frame_free(mbus_frame *frame)
{
if (frame)
{
if (frame->next != NULL)
mbus_frame_free(frame->next);
    free(frame);
    return 0;
}
return -1;

}
//------------------------------------------------------------------------------
/// Caclulate the checksum of the M-Bus frame. Internal.
//------------------------------------------------------------------------------
unsigned char
calc_checksum(mbus_frame *frame)
{
size_t i;
unsigned char cksum;
assert(frame != NULL);
switch(frame->type)
{
    case MBUS_FRAME_TYPE_SHORT:

        cksum = frame->control;
        cksum += frame->address;

        break;

    case MBUS_FRAME_TYPE_CONTROL:

        cksum = frame->control;
        cksum += frame->address;
        cksum += frame->control_information;

        break;

    case MBUS_FRAME_TYPE_LONG:

        cksum = frame->control;
        cksum += frame->address;
        cksum += frame->control_information;

        for (i = 0; i < frame->data_size; i++)
        {
            cksum += frame->data[i];
        }

        break;

    case MBUS_FRAME_TYPE_ACK:
    default:
        cksum = 0;
}

return cksum;

}
//------------------------------------------------------------------------------
/// Caclulate the checksum of the M-Bus frame. The checksum algorithm is the
/// arithmetic sum of the frame content, without using carry. Which content
/// that is included in the checksum calculation depends on the frame type.
//------------------------------------------------------------------------------
int
mbus_frame_calc_checksum(mbus_frame *frame)
{
if (frame)
{
switch (frame->type)
{
case MBUS_FRAME_TYPE_ACK:
case MBUS_FRAME_TYPE_SHORT:
case MBUS_FRAME_TYPE_CONTROL:
case MBUS_FRAME_TYPE_LONG:
frame->checksum = calc_checksum(frame);
            break;

        default:
            return -1;
    }
}

return 0;

}
///
/// Calculate the values of the lengths fields in the M-Bus frame. Internal.
///
unsigned char
calc_length(const mbus_frame *frame)
{
assert(frame != NULL);
switch(frame->type)
{
case MBUS_FRAME_TYPE_CONTROL:
return 3;
case MBUS_FRAME_TYPE_LONG:
return frame->data_size + 3;
default:
return 0;
}
}
//------------------------------------------------------------------------------
/// Calculate the values of the lengths fields in the M-Bus frame.
//------------------------------------------------------------------------------
int
mbus_frame_calc_length(mbus_frame *frame)
{
if (frame)
{
frame->length1 = frame->length2 = calc_length(frame);
}
return 0;

}
//------------------------------------------------------------------------------
/// Return the M-Bus frame type
//------------------------------------------------------------------------------
int
mbus_frame_type(mbus_frame *frame)
{
if (frame)
{
return frame->type;
}
return -1;
}
//------------------------------------------------------------------------------
/// Return the M-Bus frame direction
//------------------------------------------------------------------------------
int
mbus_frame_direction(mbus_frame *frame)
{
if (frame)
{
if (frame->type == MBUS_FRAME_TYPE_ACK)
{
return MBUS_CONTROL_MASK_DIR_S2M;
}
else
{
return (frame->control & MBUS_CONTROL_MASK_DIR);
}
}
return -1;
}
//------------------------------------------------------------------------------
/// Verify that parsed frame is a valid M-bus frame.
//
// Possible checks:
//
// 1) frame type
// 2) Start/stop bytes
// 3) control field
// 4) length field and actual data size
// 5) checksum
//
//------------------------------------------------------------------------------
int
mbus_frame_verify(mbus_frame *frame)
{
unsigned char checksum;
if (frame)
{
    switch (frame->type)
    {
        case MBUS_FRAME_TYPE_ACK:
            return frame->start1 == MBUS_FRAME_ACK_START;

        case MBUS_FRAME_TYPE_SHORT:
            if(frame->start1 != MBUS_FRAME_SHORT_START)
            {
                snprintf(error_str, sizeof(error_str), "No frame start");

                return -1;
            }

            if ((frame->control !=  MBUS_CONTROL_MASK_SND_NKE)                          &&
                (frame->control !=  MBUS_CONTROL_MASK_REQ_UD1)                          &&
                (frame->control != (MBUS_CONTROL_MASK_REQ_UD1 | MBUS_CONTROL_MASK_FCB)) &&
                (frame->control !=  MBUS_CONTROL_MASK_REQ_UD2)                          &&
                (frame->control != (MBUS_CONTROL_MASK_REQ_UD2 | MBUS_CONTROL_MASK_FCB)))
            {
                snprintf(error_str, sizeof(error_str), "Unknown Control Code 0x%.2x", frame->control);

                return -1;
            }

            break;

        case MBUS_FRAME_TYPE_CONTROL:
        case MBUS_FRAME_TYPE_LONG:
            if(frame->start1  != MBUS_FRAME_CONTROL_START ||
               frame->start2  != MBUS_FRAME_CONTROL_START)
            {
                snprintf(error_str, sizeof(error_str), "No frame start");

                return -1;
            }

            if ((frame->control !=  MBUS_CONTROL_MASK_SND_UD)                          &&
                (frame->control != (MBUS_CONTROL_MASK_SND_UD | MBUS_CONTROL_MASK_FCB)) &&
                (frame->control !=  MBUS_CONTROL_MASK_RSP_UD)                          &&
                (frame->control != (MBUS_CONTROL_MASK_RSP_UD | MBUS_CONTROL_MASK_DFC)) &&
                (frame->control != (MBUS_CONTROL_MASK_RSP_UD | MBUS_CONTROL_MASK_ACD)) &&
                (frame->control != (MBUS_CONTROL_MASK_RSP_UD | MBUS_CONTROL_MASK_DFC | MBUS_CONTROL_MASK_ACD)))
            {
                snprintf(error_str, sizeof(error_str), "Unknown Control Code 0x%.2x", frame->control);

                return -1;
            }

            if (frame->length1 != frame->length2)
            {
                snprintf(error_str, sizeof(error_str), "Frame length 1 != 2");

                return -1;
            }

            if (frame->length1 != calc_length(frame))
            {
                snprintf(error_str, sizeof(error_str), "Frame length 1 != calc length");

                return -1;
            }

            break;

        default:
            snprintf(error_str, sizeof(error_str), "Unknown frame type 0x%.2x", frame->type);

            return -1;
    }

    if(frame->stop != MBUS_FRAME_STOP)
    {
        snprintf(error_str, sizeof(error_str), "No frame stop");

        return -1;
    }

    checksum = calc_checksum(frame);

    if(frame->checksum != checksum)
    {
        snprintf(error_str, sizeof(error_str), "Invalid checksum (0x%.2x != 0x%.2x)", frame->checksum, checksum);

        return -1;
    }

    return 0;
}

snprintf(error_str, sizeof(error_str), "Got null pointer to frame.");

return -1;

}
//------------------------------------------------------------------------------
//
// DATA ENCODING, DECODING, AND CONVERSION FUNCTIONS
//
//------------------------------------------------------------------------------
//------------------------------------------------------------------------------
///
/// Encode BCD data
///
//------------------------------------------------------------------------------
int
mbus_data_bcd_encode(unsigned char *bcd_data, size_t bcd_data_size, int value)
{
int v0, v1, v2, x1, x2;
size_t i;
if (bcd_data && bcd_data_size)
{
    v2 = abs(value);

    for (i = 0; i < bcd_data_size; i++)
    {
        v0 = v2;
        v1 = (int)(v0 / 10);
        v2 = (int)(v1 / 10);

        x1 = v0 - v1 * 10;
        x2 = v1 - v2 * 10;

        bcd_data[bcd_data_size-1-i] = (x2 << 4) | x1;
    }

    if (value < 0)
    {
        bcd_data[bcd_data_size-1] |= 0xF0;
    }

    return 0;
}

return -1;

}
//------------------------------------------------------------------------------
///
/// Decode BCD data (decimal)
///
//------------------------------------------------------------------------------
long long
mbus_data_bcd_decode(unsigned char *bcd_data, size_t bcd_data_size)
{
long long val = 0;
size_t i;
if (bcd_data)
{
    for (i = bcd_data_size; i > 0; i--)
    {
        val = (val * 10);

        if (bcd_data[i-1]>>4 < 0xA)
        {
            val += ((bcd_data[i-1]>>4) & 0xF);
        }

        val = (val * 10) + ( bcd_data[i-1] & 0xF);
    }

    // hex code Fh in the MSD position signals a negative BCD number
    if (bcd_data[bcd_data_size-1]>>4 == 0xF)
    {
        val *= -1;
    }

    return val;
}

return -1;

}
//------------------------------------------------------------------------------
///
/// Decode BCD data (hexadecimal)
///
//------------------------------------------------------------------------------
long long
mbus_data_bcd_decode_hex(unsigned char *bcd_data, size_t bcd_data_size)
{
long long val = 0;
size_t i;
if (bcd_data)
{
    for (i = bcd_data_size; i > 0; i--)
    {
        val = (val << 8) | bcd_data[i-1];
    }

    return val;
}

return -1;

}
//------------------------------------------------------------------------------
///
/// Decode INTEGER data
///
//------------------------------------------------------------------------------
int
mbus_data_int_decode(unsigned char *int_data, size_t int_data_size, int *value)
{
size_t i;
int neg;
*value = 0;
if (!int_data || (int_data_size < 1))
{
    return -1;
}

neg = int_data[int_data_size-1] & 0x80;

for (i = int_data_size; i > 0; i--)
{
    if (neg)
    {
        *value = (*value << 8) + (int_data[i-1] ^ 0xFF);
    }
    else
    {
        *value = (*value << 8) + int_data[i-1];
    }
}

if (neg)
{
    *value = *value * -1 - 1;
}

return 0;

}
int
mbus_data_long_decode(unsigned char *int_data, size_t int_data_size, long *value)
{
size_t i;
int neg;
*value = 0;
if (!int_data || (int_data_size < 1))
{
    return -1;
}

neg = int_data[int_data_size-1] & 0x80;

for (i = int_data_size; i > 0; i--)
{
    if (neg)
    {
        *value = (*value << 8) + (int_data[i-1] ^ 0xFF);
    }
    else
    {
        *value = (*value << 8) + int_data[i-1];
    }
}

if (neg)
{
    *value = *value * -1 - 1;
}

return 0;

}
int
mbus_data_long_long_decode(unsigned char *int_data, size_t int_data_size, long long *value)
{
size_t i;
int neg;
*value = 0;
if (!int_data || (int_data_size < 1))
{
    return -1;
}

neg = int_data[int_data_size-1] & 0x80;

for (i = int_data_size; i > 0; i--)
{
    if (neg)
    {
        *value = (*value << 8) + (int_data[i-1] ^ 0xFF);
    }
    else
    {
        *value = (*value << 8) + int_data[i-1];
    }
}

if (neg)
{
    *value = *value * -1 - 1;
}

return 0;

}
//------------------------------------------------------------------------------
///
/// Encode INTEGER data (into 'int_data_size' bytes)
///
//------------------------------------------------------------------------------
int
mbus_data_int_encode(unsigned char *int_data, size_t int_data_size, int value)
{
int i;
if (int_data)
{
    for (i = 0; i < int_data_size; i++)
    {
        int_data[i] = (value>>(i*8)) & 0xFF;
    }

    return 0;
}

return -1;

}
//------------------------------------------------------------------------------
///
/// Decode float data
///
/// see also http://en.wikipedia.org/wiki/Single-precision_floating-point_format
///
//------------------------------------------------------------------------------
float
mbus_data_float_decode(unsigned char *float_data)
{
#ifdef _HAS_NON_IEEE754_FLOAT
float val = 0.0f;
long temp = 0, fraction;
int sign,exponent;
size_t i;
if (float_data)
{
    for (i = 4; i > 0; i--)
    {
        temp = (temp << 8) + float_data[i-1];
    }

    // first bit = sign bit
    sign     = (temp >> 31) ? -1 : 1;

    // decode 8 bit exponent
    exponent = ((temp & 0x7F800000) >> 23) - 127;

    // decode explicit 23 bit fraction
    fraction = temp & 0x007FFFFF;

    if ((exponent != -127) &&
        (exponent != 128))
    {
        // normalized value, add bit 24
        fraction |= 0x800000;
    }

    // calculate float value
    val = (float) sign * fraction * pow(2.0f, -23.0f + exponent);

    return val;
}

#else
if (float_data)
{
union {
uint32_t u32;
float f;
} data;
memcpy(&(data.u32), float_data, sizeof(uint32_t));
return data.f;
}
#endif
return -1.0f;

}
//------------------------------------------------------------------------------
///
/// Decode string data.
///
//------------------------------------------------------------------------------
void
mbus_data_str_decode(unsigned char *dst, const unsigned char *src, size_t len)
{
size_t i;
i = 0;

if (src && dst)
{
    dst[len] = '\0';
    while(len > 0) {
        dst[i++] = src[--len];
    }
}

}
//------------------------------------------------------------------------------
///
/// Decode binary data.
///
//------------------------------------------------------------------------------
void
mbus_data_bin_decode(unsigned char *dst, const unsigned char *src, size_t len, size_t max_len)
{
size_t i, pos;
i = 0;
pos = 0;

if (src && dst)
{
    while((i < len) && ((pos+3) < max_len)) {
        pos += snprintf(&dst[pos], max_len - pos, "%.2X ", src[i]);
        i++;
    }

    if (pos > 0)
    {
        // remove last space
        pos--;
    }

    dst[pos] = '\0';
}

}
//------------------------------------------------------------------------------
///
/// Decode time data
///
/// Usable for the following types:
/// I = 6 bytes (Date and time)
/// F = 4 bytes (Date and time)
/// G = 2 bytes (Date)
///
/// TODO:
/// J = 3 bytes (Time)
///
//------------------------------------------------------------------------------
void
mbus_data_tm_decode(struct tm *t, unsigned char *t_data, size_t t_data_size)
{
if (t == NULL)
{
return;
}
t->tm_sec   = 0;
t->tm_min   = 0;
t->tm_hour  = 0;
t->tm_mday  = 0;
t->tm_mon   = 0;
t->tm_year  = 0;
t->tm_wday  = 0;
t->tm_yday  = 0;
t->tm_isdst = 0;

if (t_data)
{
    if (t_data_size == 6)                // Type I = Compound CP48: Date and Time
    {
        if ((t_data[1] & 0x80) == 0)     // Time valid ?
        {
            t->tm_sec   = t_data[0] & 0x3F;
            t->tm_min   = t_data[1] & 0x3F;
            t->tm_hour  = t_data[2] & 0x1F;
            t->tm_mday  = t_data[3] & 0x1F;
            t->tm_mon   = (t_data[4] & 0x0F) - 1;
            t->tm_year  = 100 + (((t_data[3] & 0xE0) >> 5) |
                          ((t_data[4] & 0xF0) >> 1));
            t->tm_isdst = (t_data[0] & 0x40) ? 1 : 0;  // day saving time
        }
    }
    else if (t_data_size == 4)           // Type F = Compound CP32: Date and Time
    {
        if ((t_data[0] & 0x80) == 0)     // Time valid ?
        {
            t->tm_min   = t_data[0] & 0x3F;
            t->tm_hour  = t_data[1] & 0x1F;
            t->tm_mday  = t_data[2] & 0x1F;
            t->tm_mon   = (t_data[3] & 0x0F) - 1;
            t->tm_year  = 100 + (((t_data[2] & 0xE0) >> 5) |
                          ((t_data[3] & 0xF0) >> 1));
            t->tm_isdst = (t_data[1] & 0x80) ? 1 : 0;  // day saving time
        }
    }
    else if (t_data_size == 2)           // Type G: Compound CP16: Date
    {
        t->tm_mday = t_data[0] & 0x1F;
        t->tm_mon  = (t_data[1] & 0x0F) - 1;
        t->tm_year = 100 + (((t_data[0] & 0xE0) >> 5) |
                     ((t_data[1] & 0xF0) >> 1));
    }
}

}
//------------------------------------------------------------------------------
///
/// Generate manufacturer code from 2-byte encoded data
///
//------------------------------------------------------------------------------
int
mbus_data_manufacturer_encode(unsigned char *m_data, unsigned char *m_code)
{
int m_val;
if (m_data == NULL || m_code == NULL)
    return -1;

m_val = ((((int)m_code[0] - 64) & 0x001F) << 10) +
        ((((int)m_code[1] - 64) & 0x001F) << 5) +
        ((((int)m_code[2] - 64) & 0x001F));

mbus_data_int_encode(m_data, 2, m_val);

return 0;

}
//------------------------------------------------------------------------------
///
/// Generate manufacturer code from 2-byte encoded data
///
//------------------------------------------------------------------------------
const char *
mbus_decode_manufacturer(unsigned char byte1, unsigned char byte2)
{
static char m_str
4
;
int m_id;

m_str[0] = byte1;
m_str[1] = byte2;

mbus_data_int_decode(m_str, 2, &m_id);

m_str[0] = (char)(((m_id>>10) & 0x001F) + 64);
m_str[1] = (char)(((m_id>>5)  & 0x001F) + 64);
m_str[2] = (char)(((m_id)     & 0x001F) + 64);
m_str[3] = 0;

return m_str;

}
const char *
mbus_data_product_name(mbus_data_variable_header *header)
{
static char buff[128];
unsigned int manufacturer;
memset(buff, 0, sizeof(buff));

if (header)
{
    manufacturer = (header->manufacturer[1] << 8) + header->manufacturer[0];

    // please keep this list ordered by manufacturer code

    if (manufacturer == mbus_manufacturer_id("ABB"))
    {
        switch (header->version)
        {
            case 0x02:
                strcpy(buff,"ABB Delta-Meter");
                break;
            case 0x20:
                strcpy(buff,"ABB B21 113-100");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("ACW"))
    {
        switch (header->version)
        {
            case 0x09:
                strcpy(buff,"Itron CF Echo 2");
                break;
            case 0x0A:
                strcpy(buff,"Itron CF 51");
                break;
            case 0x0B:
                strcpy(buff,"Itron CF 55");
                break;
            case 0x0E:
                strcpy(buff,"Itron BM +m");
                break;
            case 0x0F:
                strcpy(buff,"Itron CF 800");
                break;
            case 0x14:
                strcpy(buff,"Itron CYBLE M-Bus 1.4");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("AMT"))
    {
        if (header->version >= 0xC0)
        {
            strcpy(buff,"Aquametro CALEC ST");
        }
        else if (header->version >= 0x80)
        {
            strcpy(buff,"Aquametro CALEC MB");
        }
        else if (header->version >= 0x40)
        {
            strcpy(buff,"Aquametro SAPHIR");
        }
        else
        {
            strcpy(buff,"Aquametro AMTRON");
        }
    }
    else if (manufacturer == mbus_manufacturer_id("BEC"))
    {
        if (header->medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY)
        {
            switch (header->version)
            {
                case 0x00:
                    strcpy(buff,"Berg DCMi");
                    break;
                case 0x07:
                    strcpy(buff,"Berg BLMi");
                    break;
            }
        }
        else if (header->medium == MBUS_VARIABLE_DATA_MEDIUM_UNKNOWN)
        {
            switch (header->version)
            {
                case 0x71:
                    strcpy(buff, "Berg BMB-10S0");
                    break;
            }
        }
    }
    else if (manufacturer == mbus_manufacturer_id("EFE"))
    {
        switch (header->version)
        {
            case 0x00:
                strcpy(buff, ((header->medium == 0x06) ? "Engelmann WaterStar" : "Engelmann / Elster SensoStar 2"));
                break;
            case 0x01:
                strcpy(buff,"Engelmann SensoStar 2C");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("ELS"))
    {
        switch (header->version)
        {
            case 0x02:
                strcpy(buff,"Elster TMP-A");
                break;
            case 0x0A:
                strcpy(buff,"Elster Falcon");
                break;
            case 0x2F:
                strcpy(buff,"Elster F96 Plus");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("ELV"))
    {
        switch (header->version)
        {
            case 0x14:
            case 0x15:
            case 0x16:
            case 0x17:
            case 0x18:
            case 0x19:
            case 0x1A:
            case 0x1B:
            case 0x1C:
            case 0x1D:
                strcpy(buff, "Elvaco CMa10");
                break;
            case 0x32:
            case 0x33:
            case 0x34:
            case 0x35:
            case 0x36:
            case 0x37:
            case 0x38:
            case 0x39:
            case 0x3A:
            case 0x3B:
                strcpy(buff,"Elvaco CMa11");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("EMH"))
    {
        switch (header->version)
        {
            case 0x00:
                strcpy(buff,"EMH DIZ");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("EMU"))
    {
        if (header->medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY)
        {
            switch (header->version)
            {
                case 0x10:
                    strcpy(buff,"EMU Professional 3/75 M-Bus");
                    break;
            }
        }
    }
    else if (manufacturer == mbus_manufacturer_id("GAV"))
    {
        if (header->medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY)
        {
            switch (header->version)
            {
                case 0x2D:
                case 0x2E:
                case 0x2F:
                case 0x30:
                    strcpy(buff,"Carlo Gavazzi EM24");
                    break;
                case 0x39:
                case 0x3A:
                    strcpy(buff,"Carlo Gavazzi EM21");
                    break;
                case 0x40:
                    strcpy(buff,"Carlo Gavazzi EM33");
                    break;
            }
        }
    }
    else if (manufacturer == mbus_manufacturer_id("GMC"))
    {
        switch (header->version)
        {
            case 0xE6:
                strcpy(buff,"GMC-I A230 EMMOD 206");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("KAM"))
    {
        switch (header->version)
        {
            case 0x01:
                strcpy(buff,"Kamstrup 382 (6850-005)");
                break;
            case 0x08:
                strcpy(buff,"Kamstrup Multical 601");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("SLB"))
    {
        switch (header->version)
        {
            case 0x02:
                strcpy(buff,"Allmess Megacontrol CF-50");
                break;
            case 0x06:
                strcpy(buff,"CF Compact / Integral MK MaXX");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("HYD"))
    {
        switch (header->version)
        {
            case 0x28:
                strcpy(buff,"ABB F95 Typ US770");
                break;
            case 0x2F:
                strcpy(buff,"Hydrometer Sharky 775");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("JAN"))
    {
        if (header->medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY)
        {
            switch (header->version)
            {
                case 0x09:
                    strcpy(buff,"Janitza UMG 96S");
                    break;
            }
        }
    }
    else if (manufacturer == mbus_manufacturer_id("LUG"))
    {
        switch (header->version)
        {
            case 0x02:
                strcpy(buff,"Landis & Gyr Ultraheat 2WR5");
                break;
            case 0x03:
                strcpy(buff,"Landis & Gyr Ultraheat 2WR6");
                break;
            case 0x04:
                strcpy(buff,"Landis & Gyr Ultraheat UH50");
                break;
            case 0x07:
                strcpy(buff,"Landis & Gyr Ultraheat T230");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("LSE"))
    {
        switch (header->version)
        {
            case 0x99:
                strcpy(buff,"Siemens WFH21");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("NZR"))
    {
        switch (header->version)
        {
            case 0x01:
                strcpy(buff,"NZR DHZ 5/63");
                break;
            case 0x50:
                strcpy(buff,"NZR IC-M2");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("RAM"))
    {
        switch (header->version)
        {
            case 0x03:
                strcpy(buff, "Rossweiner ETK/ETW Modularis");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("REL"))
    {
        switch (header->version)
        {
            case 0x08:
                strcpy(buff, "Relay PadPuls M1");
                break;
            case 0x12:
                strcpy(buff, "Relay PadPuls M4");
                break;
            case 0x20:
                strcpy(buff, "Relay Padin 4");
                break;
            case 0x30:
                strcpy(buff, "Relay AnDi 4");
                break;
            case 0x40:
                strcpy(buff, "Relay PadPuls M2");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("RKE"))
    {
        switch (header->version)
        {
            case 0x69:
                strcpy(buff,"Ista sensonic II mbus");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("SBC"))
    {
        switch (header->id_bcd[3])
        {
            case 0x10:
            case 0x19:
                strcpy(buff,"Saia-Burgess ALE3");
                break;
            case 0x11:
                strcpy(buff,"Saia-Burgess AWD3");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("SEO") || manufacturer == mbus_manufacturer_id("GTE"))
    {
        switch (header->id_bcd[3])
        {
            case 0x30:
                strcpy(buff,"Sensoco PT100");
                break;
            case 0x41:
                strcpy(buff,"Sensoco 2-NTC");
                break;
            case 0x45:
                strcpy(buff,"Sensoco Laser Light");
                break;
            case 0x48:
                strcpy(buff,"Sensoco ADIO");
                break;
            case 0x51:
            case 0x61:
                strcpy(buff,"Sensoco THU");
                break;
            case 0x80:
                strcpy(buff,"Sensoco PulseCounter for E-Meter");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("SEN"))
    {
        switch (header->version)
        {
            case 0x08:
            case 0x19:
                strcpy(buff,"Sensus PolluCom E");
                break;
            case 0x0B:
                strcpy(buff,"Sensus PolluTherm");
                break;
            case 0x0E:
                strcpy(buff,"Sensus PolluStat E");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("SON"))
    {
        switch (header->version)
        {
            case 0x0D:
                strcpy(buff,"Sontex Supercal 531");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("SPX"))
    {
        switch (header->version)
        {
            case 0x31:
            case 0x34:
                strcpy(buff,"Sensus PolluTherm");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("SVM"))
    {
        switch (header->version)
        {
            case 0x08:
                strcpy(buff,"Elster F2 / Deltamess F2");
                break;
            case 0x09:
                strcpy(buff,"Elster F4 / Kamstrup SVM F22");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("TCH"))
    {
        switch (header->version)
        {
            case 0x26:
                strcpy(buff,"Techem m-bus S");
                break;
            case 0x40:
                strcpy(buff,"Techem ultra S3");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("WZG"))
    {
        switch (header->version)
        {
            case 0x03:
                strcpy(buff,"Modularis ETW-EAX");
                break;
        }
    }
    else if (manufacturer == mbus_manufacturer_id("ZRM"))
    {
        switch (header->version)
        {
            case 0x81:
                strcpy(buff,"Minol Minocal C2");
                break;
            case 0x82:
                strcpy(buff,"Minol Minocal WR3");
                break;
        }
    }

}

return buff;

}
//------------------------------------------------------------------------------
//
// FIXED-LENGTH DATA RECORD FUNCTIONS
//
//------------------------------------------------------------------------------
//
// Value Field Medium/Unit Medium
// hexadecimal Bit 16 Bit 15 Bit 8 Bit 7
// 0 0 0 0 0 Other
// 1 0 0 0 1 Oil
// 2 0 0 1 0 Electricity
// 3 0 0 1 1 Gas
// 4 0 1 0 0 Heat
// 5 0 1 0 1 Steam
// 6 0 1 1 0 Hot Water
// 7 0 1 1 1 Water
// 8 1 0 0 0 H.C.A.
// 9 1 0 0 1 Reserved
// A 1 0 1 0 Gas Mode 2
// B 1 0 1 1 Heat Mode 2
// C 1 1 0 0 Hot Water Mode 2
// D 1 1 0 1 Water Mode 2
// E 1 1 1 0 H.C.A. Mode 2
// F 1 1 1 1 Reserved
//
///
/// For fixed-length frames, get a string describing the medium.
///
const char *
mbus_data_fixed_medium(mbus_data_fixed *data)
{
static char buff[256];
if (data)
{
    switch ( (data->cnt1_type&0xC0)>>6 | (data->cnt2_type&0xC0)>>4 )
    {
        case 0x00:
            snprintf(buff, sizeof(buff), "Other");
            break;
        case 0x01:
            snprintf(buff, sizeof(buff), "Oil");
            break;
        case 0x02:
            snprintf(buff, sizeof(buff), "Electricity");
            break;
        case 0x03:
            snprintf(buff, sizeof(buff), "Gas");
            break;
        case 0x04:
            snprintf(buff, sizeof(buff), "Heat");
            break;
        case 0x05:
            snprintf(buff, sizeof(buff), "Steam");
            break;
        case 0x06:
            snprintf(buff, sizeof(buff), "Hot Water");
            break;
        case 0x07:
            snprintf(buff, sizeof(buff), "Water");
            break;
        case 0x08:
            snprintf(buff, sizeof(buff), "H.C.A.");
            break;
        case 0x09:
            snprintf(buff, sizeof(buff), "Reserved");
            break;
        case 0x0A:
            snprintf(buff, sizeof(buff), "Gas Mode 2");
            break;
        case 0x0B:
            snprintf(buff, sizeof(buff), "Heat Mode 2");
            break;
        case 0x0C:
            snprintf(buff, sizeof(buff), "Hot Water Mode 2");
            break;
        case 0x0D:
            snprintf(buff, sizeof(buff), "Water Mode 2");
            break;
        case 0x0E:
            snprintf(buff, sizeof(buff), "H.C.A. Mode 2");
            break;
        case 0x0F:
            snprintf(buff, sizeof(buff), "Reserved");
            break;
        default:
            snprintf(buff, sizeof(buff), "unknown");
            break;
    }

    return buff;
}

return NULL;

}
//------------------------------------------------------------------------------
// Hex code Hex code
//Unit share Unit share
// MSB..LSB MSB..LSB
// Byte 7/8 Byte 7/8
// h,m,s 000000 00 MJ/h 100000 20
// D,M,Y 000001 01 MJ/h * 10 100001 21
// Wh 000010 02 MJ/h * 100 100010 22
// Wh * 10 000011 03 GJ/h 100011 23
// Wh * 100 000100 04 GJ/h * 10 100100 24
// kWh 000101 05 GJ/h * 100 100101 25
// kWh * 10 000110 06 ml 100110 26
// kWh * 100 000111 07 ml * 10 100111 27
// MWh 001000 08 ml * 100 101000 28
// MWh * 10 001001 09 l 101001 29
// MWh * 100 001010 0A l * 10 101010 2A
// kJ 001011 0B l * 100 101011 2B
// kJ * 10 001100 0C m3 101100 2C
// kJ * 100 001101 0D m3 * 10 101101 2D
// MJ 001110 0E m3 * 100 101110 2E
// MJ * 10 001111 0F ml/h 101111 2F
// MJ * 100 010000 10 ml/h * 10 110000 30
// GJ 010001 11 ml/h * 100 110001 31
// GJ * 10 010010 12 l/h 110010 32
// GJ * 100 010011 13 l/h * 10 110011 33
// W 010100 14 l/h * 100 110100 34
// W * 10 010101 15 m3/h 110101 35
// W * 100 010110 16 m3/h * 10 110110 36
// kW 010111 17 m3/h * 100 110111 37
// kW * 10 011000 18 °C* 10-3 111000 38
// kW * 100 011001 19 units for HCA 111001 39
// MW 011010 1A reserved 111010 3A
// MW * 10 011011 1B reserved 111011 3B
// MW * 100 011100 1C reserved 111100 3C
// kJ/h 011101 1D reserved 111101 3D
// kJ/h * 10 011110 1E same but historic 111110 3E
// kJ/h * 100 011111 1F without units 111111 3F
//
//------------------------------------------------------------------------------
///
/// For fixed-length frames, get a string describing the unit of the data.
///
const char *
mbus_data_fixed_unit(int medium_unit_byte)
{
static char buff[256];
switch (medium_unit_byte & 0x3F)
{
    case 0x00:
        snprintf(buff, sizeof(buff), "h,m,s");
        break;
    case 0x01:
        snprintf(buff, sizeof(buff), "D,M,Y");
        break;

    case 0x02:
        snprintf(buff, sizeof(buff), "Wh");
        break;
    case 0x03:
        snprintf(buff, sizeof(buff), "10 Wh");
        break;
    case 0x04:
        snprintf(buff, sizeof(buff), "100 Wh");
        break;
    case 0x05:
        snprintf(buff, sizeof(buff), "kWh");
        break;
    case 0x06:
        snprintf(buff, sizeof(buff), "10 kWh");
        break;
    case 0x07:
        snprintf(buff, sizeof(buff), "100 kWh");
        break;
    case 0x08:
        snprintf(buff, sizeof(buff), "MWh");
        break;
    case 0x09:
        snprintf(buff, sizeof(buff), "10 MWh");
        break;
    case 0x0A:
        snprintf(buff, sizeof(buff), "100 MWh");
        break;

    case 0x0B:
        snprintf(buff, sizeof(buff), "kJ");
        break;
    case 0x0C:
        snprintf(buff, sizeof(buff), "10 kJ");
        break;
    case 0x0E:
        snprintf(buff, sizeof(buff), "100 kJ");
        break;
    case 0x0D:
        snprintf(buff, sizeof(buff), "MJ");
        break;
    case 0x0F:
        snprintf(buff, sizeof(buff), "10 MJ");
        break;
    case 0x10:
        snprintf(buff, sizeof(buff), "100 MJ");
        break;
    case 0x11:
        snprintf(buff, sizeof(buff), "GJ");
        break;
    case 0x12:
        snprintf(buff, sizeof(buff), "10 GJ");
        break;
    case 0x13:
        snprintf(buff, sizeof(buff), "100 GJ");
        break;

    case 0x14:
        snprintf(buff, sizeof(buff), "W");
        break;
    case 0x15:
        snprintf(buff, sizeof(buff), "10 W");
        break;
    case 0x16:
        snprintf(buff, sizeof(buff), "100 W");
        break;
    case 0x17:
        snprintf(buff, sizeof(buff), "kW");
        break;
    case 0x18:
        snprintf(buff, sizeof(buff), "10 kW");
        break;
    case 0x19:
        snprintf(buff, sizeof(buff), "100 kW");
        break;
    case 0x1A:
        snprintf(buff, sizeof(buff), "MW");
        break;
    case 0x1B:
        snprintf(buff, sizeof(buff), "10 MW");
        break;
    case 0x1C:
        snprintf(buff, sizeof(buff), "100 MW");
        break;

    case 0x1D:
        snprintf(buff, sizeof(buff), "kJ/h");
        break;
    case 0x1E:
        snprintf(buff, sizeof(buff), "10 kJ/h");
        break;
    case 0x1F:
        snprintf(buff, sizeof(buff), "100 kJ/h");
        break;
    case 0x20:
        snprintf(buff, sizeof(buff), "MJ/h");
        break;
    case 0x21:
        snprintf(buff, sizeof(buff), "10 MJ/h");
        break;
    case 0x22:
        snprintf(buff, sizeof(buff), "100 MJ/h");
        break;
    case 0x23:
        snprintf(buff, sizeof(buff), "GJ/h");
        break;
    case 0x24:
        snprintf(buff, sizeof(buff), "10 GJ/h");
        break;
    case 0x25:
        snprintf(buff, sizeof(buff), "100 GJ/h");
        break;

    case 0x26:
        snprintf(buff, sizeof(buff), "ml");
        break;
    case 0x27:
        snprintf(buff, sizeof(buff), "10 ml");
        break;
    case 0x28:
        snprintf(buff, sizeof(buff), "100 ml");
        break;
    case 0x29:
        snprintf(buff, sizeof(buff), "l");
        break;
    case 0x2A:
        snprintf(buff, sizeof(buff), "10 l");
        break;
    case 0x2B:
        snprintf(buff, sizeof(buff), "100 l");
        break;
    case 0x2C:
        snprintf(buff, sizeof(buff), "m^3");
        break;
    case 0x2D:
        snprintf(buff, sizeof(buff), "10 m^3");
        break;
    case 0x2E:
        snprintf(buff, sizeof(buff), "100 m^3");
        break;

    case 0x2F:
        snprintf(buff, sizeof(buff), "ml/h");
        break;
    case 0x30:
        snprintf(buff, sizeof(buff), "10 ml/h");
        break;
    case 0x31:
        snprintf(buff, sizeof(buff), "100 ml/h");
        break;
    case 0x32:
        snprintf(buff, sizeof(buff), "l/h");
        break;
    case 0x33:
        snprintf(buff, sizeof(buff), "10 l/h");
        break;
    case 0x34:
        snprintf(buff, sizeof(buff), "100 l/h");
        break;
    case 0x35:
        snprintf(buff, sizeof(buff), "m^3/h");
        break;
    case 0x36:
        snprintf(buff, sizeof(buff), "10 m^3/h");
        break;
    case 0x37:
        snprintf(buff, sizeof(buff), "100 m^3/h");
        break;

    case 0x38:
        snprintf(buff, sizeof(buff), "1e-3 °C");
        break;
    case 0x39:
        snprintf(buff, sizeof(buff), "units for HCA");
        break;
    case 0x3A:
    case 0x3B:
    case 0x3C:
    case 0x3D:
        snprintf(buff, sizeof(buff), "reserved");
        break;
    case 0x3E:
        snprintf(buff, sizeof(buff), "reserved but historic");
        break;
    case 0x3F:
        snprintf(buff, sizeof(buff), "without units");
        break;
    default:
        snprintf(buff, sizeof(buff), "unknown");
        break;
}

return buff;

}
//------------------------------------------------------------------------------
//
// VARIABLE-LENGTH DATA RECORD FUNCTIONS
//
//------------------------------------------------------------------------------
//------------------------------------------------------------------------------
//
// Medium Code bin Code hex
// Other 0000 0000 00
// Oil 0000 0001 01
// Electricity 0000 0010 02
// Gas 0000 0011 03
// Heat (Volume measured at return temperature: outlet) 0000 0100 04
// Steam 0000 0101 05
// Hot Water 0000 0110 06
// Water 0000 0111 07
// Heat Cost Allocator. 0000 1000 08
// Compressed Air 0000 1001 09
// Cooling load meter (Volume measured at return temperature: outlet) 0000 1010 0A
// Cooling load meter (Volume measured at flow temperature: inlet) ♣ 0000 1011 0B
// Heat (Volume measured at flow temperature: inlet) 0000 1100 0C
// Heat / Cooling load meter ♣ 0000 1101 OD
// Bus / System 0000 1110 0E
// Unknown Medium 0000 1111 0F
// Irrigation Water (Non Drinkable) 0001 0000 10
// Water data logger 0001 0001 11
// Gas data logger 0001 0010 12
// Gas converter 0001 0011 13
// Heat Value 0001 0100 14
// Hot Water (>=90°C) (Non Drinkable) 0001 0101 15
// Cold Water 0001 0110 16
// Dual Water 0001 0111 17
// Pressure 0001 1000 18
// A/D Converter 0001 1001 19
// Smoke detector 0001 1010 1A
// Room sensor (e.g. Temperature or Humidity) 0001 1011 1B
// Gas detector 0001 1100 1C
// Reserved for Sensors .......... 1D to 1F
// Breaker (Electricity) 0010 0000 20
// Valve (Gas or Water) 0010 0001 21
// Reserved for Switching Units .......... 22 to 24
// Customer Unit (Display) 0010 0101 25
// Reserved for End User Units .......... 26 to 27
// Waste Water (Non Drinkable) 0010 1000 28
// Waste 0010 1001 29
// Reserved for CO2 0010 1010 2A
// Reserved for environmental meter .......... 2B to 2F
// Service tool 0011 0000 30
// Gateway 0011 0001 31
// Unidirectional Repeater 0011 0010 32
// Bidirectional Repeater 0011 0011 33
// Reserved for System Units .......... 34 to 35
// Radio Control Unit (System Side) 0011 0110 36
// Radio Control Unit (Meter Side) 0011 0111 37
// Bus Control Unit (Meter Side) 0011 1000 38
// Reserved for System Units .......... 38 to 3F
// Reserved .......... 40 to FE
// Placeholder 1111 1111 FF
//------------------------------------------------------------------------------
///
/// For variable-length frames, returns a string describing the medium.
///
const char *
mbus_data_variable_medium_lookup(unsigned char medium)
{
static char buff[256];
switch (medium)
{
    case MBUS_VARIABLE_DATA_MEDIUM_OTHER:
        snprintf(buff, sizeof(buff), "Other");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_OIL:
        snprintf(buff, sizeof(buff), "Oil");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY:
        snprintf(buff, sizeof(buff), "Electricity");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_GAS:
        snprintf(buff, sizeof(buff), "Gas");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_HEAT_OUT:
        snprintf(buff, sizeof(buff), "Heat: Outlet");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_STEAM:
        snprintf(buff, sizeof(buff), "Steam");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_HOT_WATER:
        snprintf(buff, sizeof(buff), "Warm water (30-90°C)");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_WATER:
        snprintf(buff, sizeof(buff), "Water");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_HEAT_COST:
        snprintf(buff, sizeof(buff), "Heat Cost Allocator");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_COMPR_AIR:
        snprintf(buff, sizeof(buff), "Compressed Air");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_COOL_OUT:
        snprintf(buff, sizeof(buff), "Cooling load meter: Outlet");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_COOL_IN:
        snprintf(buff, sizeof(buff), "Cooling load meter: Inlet");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_HEAT_IN:
        snprintf(buff, sizeof(buff), "Heat: Inlet");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_HEAT_COOL:
        snprintf(buff, sizeof(buff), "Heat / Cooling load meter");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_BUS:
        snprintf(buff, sizeof(buff), "Bus/System");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_UNKNOWN:
        snprintf(buff, sizeof(buff), "Unknown Medium");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_IRRIGATION:
        snprintf(buff, sizeof(buff), "Irrigation Water");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_WATER_LOGGER:
        snprintf(buff, sizeof(buff), "Water Logger");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_GAS_LOGGER:
        snprintf(buff, sizeof(buff), "Gas Logger");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_GAS_CONV:
        snprintf(buff, sizeof(buff), "Gas Converter");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_COLORIFIC:
        snprintf(buff, sizeof(buff), "Calorific value");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_BOIL_WATER:
        snprintf(buff, sizeof(buff), "Hot water (>90°C)");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_COLD_WATER:
        snprintf(buff, sizeof(buff), "Cold water");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_DUAL_WATER:
        snprintf(buff, sizeof(buff), "Dual water");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_PRESSURE:
        snprintf(buff, sizeof(buff), "Pressure");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_ADC:
        snprintf(buff, sizeof(buff), "A/D Converter");
        break;

    case MBUS_VARIABLE_DATA_MEDIUM_SMOKE:
      snprintf(buff, sizeof(buff), "Smoke Detector");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_ROOM_SENSOR:
      snprintf(buff, sizeof(buff), "Ambient Sensor");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_GAS_DETECTOR:
      snprintf(buff, sizeof(buff), "Gas Detector");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_BREAKER_E:
      snprintf(buff, sizeof(buff), "Breaker: Electricity");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_VALVE:
      snprintf(buff, sizeof(buff), "Valve: Gas or Water");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_CUSTOMER_UNIT:
      snprintf(buff, sizeof(buff), "Customer Unit: Display Device");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_WASTE_WATER:
      snprintf(buff, sizeof(buff), "Waste Water");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_GARBAGE:
      snprintf(buff, sizeof(buff), "Garbage");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_SERVICE_UNIT:
      snprintf(buff, sizeof(buff), "Service Unit");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_RC_SYSTEM:
      snprintf(buff, sizeof(buff), "Radio Converter: System");
      break;

    case MBUS_VARIABLE_DATA_MEDIUM_RC_METER:
      snprintf(buff, sizeof(buff), "Radio Converter: Meter");
      break;

    case 0x22:
    case 0x23:
    case 0x24:
    case 0x26:
    case 0x27:
    case 0x2A:
    case 0x2B:
    case 0x2C:
    case 0x2D:
    case 0x2E:
    case 0x2F:
    case 0x31:
    case 0x32:
    case 0x33:
    case 0x34:
    case 0x38:
    case 0x39:
    case 0x3A:
    case 0x3B:
    case 0x3C:
    case 0x3D:
    case 0x3E:
    case 0x3F:
        snprintf(buff, sizeof(buff), "Reserved");
        break;


    // add more ...
    default:
        snprintf(buff, sizeof(buff), "Unknown medium (0x%.2x)", medium);
        break;
}

return buff;

}
//------------------------------------------------------------------------------
///
/// Lookup the unit description from a VIF field in a data record
///
//------------------------------------------------------------------------------
const char *
mbus_unit_prefix(int exp)
{
static char buff[256];
switch (exp)
{
    case 0:
        buff[0] = 0;
        break;

    case -3:
        snprintf(buff, sizeof(buff), "m");
        break;

    case -6:
        snprintf(buff, sizeof(buff), "my");
        break;

    case 1:
        snprintf(buff, sizeof(buff), "10 ");
        break;

    case 2:
        snprintf(buff, sizeof(buff), "100 ");
        break;

    case 3:
        snprintf(buff, sizeof(buff), "k");
        break;

    case 4:
        snprintf(buff, sizeof(buff), "10 k");
        break;

    case 5:
        snprintf(buff, sizeof(buff), "100 k");
        break;

    case 6:
        snprintf(buff, sizeof(buff), "M");
        break;

    case 9:
        snprintf(buff, sizeof(buff), "G");
        break;

    default:
        snprintf(buff, sizeof(buff), "1e%d ", exp);
}

return buff;

}
//------------------------------------------------------------------------------
/// Look up the data length from a DIF field in the data record.
///
/// See the table on page 41 the M-BUS specification.
//------------------------------------------------------------------------------
unsigned char
mbus_dif_datalength_lookup(unsigned char dif)
{
switch (dif & MBUS_DATA_RECORD_DIF_MASK_DATA)
{
case 0x0:
return 0;
case 0x1:
return 1;
case 0x2:
return 2;
case 0x3:
return 3;
case 0x4:
return 4;
case 0x5:
return 4;
case 0x6:
return 6;
case 0x7:
return 8;
case 0x8:
return 0;
case 0x9:
return 1;
case 0xA:
return 2;
case 0xB:
return 3;
case 0xC:
return 4;
case 0xD:
// variable data length,
// data length stored in data field
return 0;
case 0xE:
return 6;
case 0xF:
return 8;
    default: // never reached
        return 0x00;

}

}
//------------------------------------------------------------------------------
/// Look up the unit from a VIF field in the data record.
///
/// See section 8.4.3 Codes for Value Information Field (VIF) in the M-BUS spec
//------------------------------------------------------------------------------
const char *
mbus_vif_unit_lookup(unsigned char vif)
{
static char buff[256];
int n;
switch (vif & MBUS_DIB_VIF_WITHOUT_EXTENSION) // ignore the extension bit in this selection
{
    // E000 0nnn Energy 10(nnn-3) W
    case 0x00:
    case 0x00+1:
    case 0x00+2:
    case 0x00+3:
    case 0x00+4:
    case 0x00+5:
    case 0x00+6:
    case 0x00+7:
        n = (vif & 0x07) - 3;
        snprintf(buff, sizeof(buff), "Energy (%sWh)", mbus_unit_prefix(n));
        break;

    // 0000 1nnn          Energy       10(nnn)J     (0.001kJ to 10000kJ)
    case 0x08:
    case 0x08+1:
    case 0x08+2:
    case 0x08+3:
    case 0x08+4:
    case 0x08+5:
    case 0x08+6:
    case 0x08+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Energy (%sJ)", mbus_unit_prefix(n));

        break;

    // E001 1nnn Mass 10(nnn-3) kg 0.001kg to 10000kg
    case 0x18:
    case 0x18+1:
    case 0x18+2:
    case 0x18+3:
    case 0x18+4:
    case 0x18+5:
    case 0x18+6:
    case 0x18+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Mass (%skg)", mbus_unit_prefix(n-3));

        break;

    // E010 1nnn Power 10(nnn-3) W 0.001W to 10000W
    case 0x28:
    case 0x28+1:
    case 0x28+2:
    case 0x28+3:
    case 0x28+4:
    case 0x28+5:
    case 0x28+6:
    case 0x28+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Power (%sW)", mbus_unit_prefix(n-3));
        //snprintf(buff, sizeof(buff), "Power (10^%d W)", n-3);

        break;

    // E011 0nnn Power 10(nnn) J/h 0.001kJ/h to 10000kJ/h
    case 0x30:
    case 0x30+1:
    case 0x30+2:
    case 0x30+3:
    case 0x30+4:
    case 0x30+5:
    case 0x30+6:
    case 0x30+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Power (%sJ/h)", mbus_unit_prefix(n));

        break;

    // E001 0nnn Volume 10(nnn-6) m3 0.001l to 10000l
    case 0x10:
    case 0x10+1:
    case 0x10+2:
    case 0x10+3:
    case 0x10+4:
    case 0x10+5:
    case 0x10+6:
    case 0x10+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Volume (%s m^3)", mbus_unit_prefix(n-6));

        break;

    // E011 1nnn Volume Flow 10(nnn-6) m3/h 0.001l/h to 10000l/
    case 0x38:
    case 0x38+1:
    case 0x38+2:
    case 0x38+3:
    case 0x38+4:
    case 0x38+5:
    case 0x38+6:
    case 0x38+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Volume flow (%s m^3/h)", mbus_unit_prefix(n-6));

        break;

    // E100 0nnn Volume Flow ext. 10(nnn-7) m3/min 0.0001l/min to 1000l/min
    case 0x40:
    case 0x40+1:
    case 0x40+2:
    case 0x40+3:
    case 0x40+4:
    case 0x40+5:
    case 0x40+6:
    case 0x40+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Volume flow (%s m^3/min)", mbus_unit_prefix(n-7));

        break;

    // E100 1nnn Volume Flow ext. 10(nnn-9) m3/s 0.001ml/s to 10000ml/
    case 0x48:
    case 0x48+1:
    case 0x48+2:
    case 0x48+3:
    case 0x48+4:
    case 0x48+5:
    case 0x48+6:
    case 0x48+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Volume flow (%s m^3/s)", mbus_unit_prefix(n-9));

        break;

    // E101 0nnn Mass flow 10(nnn-3) kg/h 0.001kg/h to 10000kg/
    case 0x50:
    case 0x50+1:
    case 0x50+2:
    case 0x50+3:
    case 0x50+4:
    case 0x50+5:
    case 0x50+6:
    case 0x50+7:

        n = (vif & 0x07);
        snprintf(buff, sizeof(buff), "Mass flow (%s kg/h)", mbus_unit_prefix(n-3));

        break;

    // E101 10nn Flow Temperature 10(nn-3) °C 0.001°C to 1°C
    case 0x58:
    case 0x58+1:
    case 0x58+2:
    case 0x58+3:

        n = (vif & 0x03);
        snprintf(buff, sizeof(buff), "Flow temperature (%sdeg C)", mbus_unit_prefix(n-3));

        break;

    // E101 11nn Return Temperature 10(nn-3) °C 0.001°C to 1°C
    case 0x5C:
    case 0x5C+1:
    case 0x5C+2:
    case 0x5C+3:

        n = (vif & 0x03);
        snprintf(buff, sizeof(buff), "Return temperature (%sdeg C)", mbus_unit_prefix(n-3));

        break;

    // E110 10nn Pressure 10(nn-3) bar 1mbar to 1000mbar
    case 0x68:
    case 0x68+1:
    case 0x68+2:
    case 0x68+3:

        n = (vif & 0x03);
        snprintf(buff, sizeof(buff), "Pressure (%s bar)", mbus_unit_prefix(n-3));

        break;

    // E010 00nn On Time
    // nn = 00 seconds
    // nn = 01 minutes
    // nn = 10   hours
    // nn = 11    days
    // E010 01nn Operating Time coded like OnTime
    // E111 00nn Averaging Duration coded like OnTime
    // E111 01nn Actuality Duration coded like OnTime
    case 0x20:
    case 0x20+1:
    case 0x20+2:
    case 0x20+3:
    case 0x24:
    case 0x24+1:
    case 0x24+2:
    case 0x24+3:
    case 0x70:
    case 0x70+1:
    case 0x70+2:
    case 0x70+3:
    case 0x74:
    case 0x74+1:
    case 0x74+2:
    case 0x74+3:
        {
            int offset;

            if      ((vif & 0x7C) == 0x20)
                offset = snprintf(buff, sizeof(buff), "On time ");
            else if ((vif & 0x7C) == 0x24)
                offset = snprintf(buff, sizeof(buff), "Operating time ");
            else if ((vif & 0x7C) == 0x70)
                offset = snprintf(buff, sizeof(buff), "Averaging Duration ");
            else
                offset = snprintf(buff, sizeof(buff), "Actuality Duration ");

            switch (vif & 0x03)
            {
                case 0x00:
                    snprintf(&buff[offset], sizeof(buff)-offset, "(seconds)");
                    break;
                case 0x01:
                    snprintf(&buff[offset], sizeof(buff)-offset, "(minutes)");
                    break;
                case 0x02:
                    snprintf(&buff[offset], sizeof(buff)-offset, "(hours)");
                    break;
                case 0x03:
                    snprintf(&buff[offset], sizeof(buff)-offset, "(days)");
                    break;
            }
        }
        break;

    // E110 110n Time Point
    // n = 0        date
    // n = 1 time & date
    // data type G
    // data type F
    case 0x6C:
    case 0x6C+1:

        if (vif & 0x1)
            snprintf(buff, sizeof(buff), "Time Point (time & date)");
        else
            snprintf(buff, sizeof(buff), "Time Point (date)");

        break;

    // E110 00nn    Temperature Difference   10(nn-3)K   (mK to  K)
    case 0x60:
    case 0x60+1:
    case 0x60+2:
    case 0x60+3:

        n = (vif & 0x03);

        snprintf(buff, sizeof(buff), "Temperature Difference (%s deg C)", mbus_unit_prefix(n-3));

        break;

    // E110 01nn External Temperature 10(nn-3) °C 0.001°C to 1°C
    case 0x64:
    case 0x64+1:
    case 0x64+2:
    case 0x64+3:

        n = (vif & 0x03);
        snprintf(buff, sizeof(buff), "External temperature (%s deg C)", mbus_unit_prefix(n-3));

        break;

    // E110 1110 Units for H.C.A. dimensionless
    case 0x6E:
        snprintf(buff, sizeof(buff), "Units for H.C.A.");
        break;

    // E110 1111 Reserved
    case 0x6F:
        snprintf(buff, sizeof(buff), "Reserved");
        break;

    // Custom VIF in the following string: never reached...
    case 0x7C:
        snprintf(buff, sizeof(buff), "Custom VIF");
        break;

    // Fabrication No
    case 0x78:
        snprintf(buff, sizeof(buff), "Fabrication number");
        break;

    // Bus Address
    case 0x7A:
        snprintf(buff, sizeof(buff), "Bus Address");
        break;

    // Manufacturer specific: 7Fh / FF
    case 0x7F:
    case 0xFF:
        snprintf(buff, sizeof(buff), "Manufacturer specific");
        break;

    default:
        snprintf(buff, sizeof(buff), "Unknown (VIF=0x%.2X)", vif);
        break;
}


return buff;

}
//------------------------------------------------------------------------------
// Lookup the error message
//
// See section 6.6 Codes for general application errors in the M-BUS spec
//------------------------------------------------------------------------------
const char *
mbus_data_error_lookup(int error)
{
static char buff[256];
switch (error)
{
    case MBUS_ERROR_DATA_UNSPECIFIED:
        snprintf(buff, sizeof(buff), "Unspecified error");
        break;

    case MBUS_ERROR_DATA_UNIMPLEMENTED_CI:
        snprintf(buff, sizeof(buff), "Unimplemented CI-Field");
        break;

    case MBUS_ERROR_DATA_BUFFER_TOO_LONG:
        snprintf(buff, sizeof(buff), "Buffer too long, truncated");
        break;

    case MBUS_ERROR_DATA_TOO_MANY_RECORDS:
        snprintf(buff, sizeof(buff), "Too many records");
        break;

    case MBUS_ERROR_DATA_PREMATURE_END:
        snprintf(buff, sizeof(buff), "Premature end of record");
        break;

    case MBUS_ERROR_DATA_TOO_MANY_DIFES:
        snprintf(buff, sizeof(buff), "More than 10 DIFE´s");
        break;

    case MBUS_ERROR_DATA_TOO_MANY_VIFES:
        snprintf(buff, sizeof(buff), "More than 10 VIFE´s");
        break;

    case MBUS_ERROR_DATA_RESERVED:
        snprintf(buff, sizeof(buff), "Reserved");
        break;

    case MBUS_ERROR_DATA_APPLICATION_BUSY:
        snprintf(buff, sizeof(buff), "Application busy");
        break;

    case MBUS_ERROR_DATA_TOO_MANY_READOUTS:
        snprintf(buff, sizeof(buff), "Too many readouts");
        break;

    default:
        snprintf(buff, sizeof(buff), "Unknown error (0x%.2X)", error);
        break;
}

return buff;

}
static const char *
mbus_unit_duration_nn(int nn)
{
switch (nn)
{
case 0:
return "second(s)";
case 1:
return "minute(s)";
case 2:
return "hour(s)";
case 3:
return "day(s)";
}
return "error: out-of-range";
}
static const char *
mbus_unit_duration_pp(int pp)
{
switch (pp)
{
case 0:
return "hour(s)";
    case 1:
        return "day(s)";

    case 2:
        return "month(s)";

    case 3:
        return "year(s)";
}
return "error: out-of-range";

}
static const char *
mbus_vib_unit_lookup_fb(mbus_value_information_block *vib)
{
static char buff[256];
int n;
const char * prefix = "";
switch (vib->vife
0
 & MBUS_DIB_VIF_WITHOUT_EXTENSION)
{
case 0x0:
case 0x0 + 1:
// E000 000n
n = 0x01 & vib->vife
0
;
if (n == 0)
prefix = "0.1 ";
snprintf(buff, sizeof(buff), "Energy (%sMWh)", prefix);
break;
case 0x2:
case 0x2 + 1:
// E000 001n
case 0x4:
case 0x4 + 1:
case 0x4 + 2:
case 0x4 + 3:
// E000 01nn
snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife
0
);
break;
case 0x8:
case 0x8 + 1:
// E000 100n
n = 0x01 & vib->vife
0
;
if (n == 0)
prefix = "0.1 ";
snprintf(buff, sizeof(buff), "Energy (%sGJ)", prefix);
break;
case 0xA:
case 0xA + 1:
case 0xC:
case 0xC + 1:
case 0xC + 2:
case 0xC + 3:
// E000 101n
// E000 11nn
snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife
0
);
break;
case 0x10:
case 0x10 + 1:
// E001 000n
n = 0x01 & vib->vife
0
;
snprintf(buff, sizeof(buff), "Volume (%sm3)", mbus_unit_prefix(n+2));
break;
case 0x12:
case 0x12 + 1:
case 0x14:
case 0x14 + 1:
case 0x14 + 2:
case 0x14 + 3:
// E001 001n
// E001 01nn
snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife
0
);
break;
case 0x18:
case 0x18 + 1:
// E001 100n
n = 0x01 & vib->vife
0
;
snprintf(buff, sizeof(buff), "Mass (%st)", mbus_unit_prefix(n+2));
break;
case 0x1A:
case 0x1B:
case 0x1C:
case 0x1D:
case 0x1E:
case 0x1F:
case 0x20:
// E001 1010 to E010 0000, Reserved
snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife
0
);
break;
case 0x21:
// E010 0001
snprintf(buff, sizeof(buff), "Volume (0.1 feet^3)");
break;
case 0x22:
case 0x23:
// E010 0010
// E010 0011
n = 0x01 & vib->vife
0
;
if (n == 0)
prefix = "0.1 ";
snprintf(buff, sizeof(buff), "Volume (%samerican gallon)", prefix);
break;
case 0x24:
    // E010 0100
    snprintf(buff, sizeof(buff), "Volume flow (0.001 american gallon/min)");
    break;
case 0x25:
    // E010 0101
    snprintf(buff, sizeof(buff), "Volume flow (american gallon/min)");
    break;
case 0x26:
    // E010 0110
    snprintf(buff, sizeof(buff), "Volume flow (american gallon/h)");
    break;
case 0x27:
    // E010 0111, Reserved
    snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife[0]);
    break;
case 0x28:
case 0x28 + 1:
    // E010 100n
    n = 0x01 & vib->vife[0];
    if (n == 0)
       prefix = "0.1 ";
    snprintf(buff, sizeof(buff), "Power (%sMW)", prefix);
    break;
case 0x2A:
case 0x2A + 1:
case 0x2C:
case 0x2C + 1:
case 0x2C + 2:
case 0x2C + 3:
    // E010 101n, Reserved
    // E010 11nn, Reserved
    snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife[0]);
    break;
case 0x30:
case 0x30 + 1:
    // E011 000n
    n = 0x01 & vib->vife[0];
    if (n == 0)
       prefix = "0.1 ";
    snprintf(buff, sizeof(buff), "Power (%sGJ/h)", prefix);
    break;
case 0x32:
case 0x33:
case 0x34:
case 0x35:
case 0x36:
case 0x37:
case 0x38:
case 0x39:
case 0x3A:
case 0x3B:
case 0x3C:
case 0x3D:
case 0x3E:
case 0x3F:

case 0x40:
case 0x41:
case 0x42:
case 0x43:
case 0x44:
case 0x45:
case 0x46:
case 0x47:
case 0x48:
case 0x49:
case 0x4A:
case 0x4B:
case 0x4C:
case 0x4D:
case 0x4E:
case 0x4F:

case 0x52:
case 0x53:
case 0x54:
case 0x55:
case 0x56:
case 0x57:
    // E011 0010 to E101 0111
    snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife[0]);
    break;
case 0x58:
case 0x58 + 1:
case 0x58 + 2:
case 0x58 + 3:
    // E101 10nn
    n = 0x03 & vib->vife[0];
    snprintf(buff, sizeof(buff), "Flow Temperature (%s degree F)", mbus_unit_prefix(n -3));
    break;
case 0x5C:
case 0x5C + 1:
case 0x5C + 2:
case 0x5C + 3:
    // E101 11nn
    n = 0x03 & vib->vife[0];
    snprintf(buff, sizeof(buff), "Return Temperature (%s degree F)", mbus_unit_prefix(n -3));
    break;
case 0x60:
case 0x60 + 1:
case 0x60 + 2:
case 0x60 + 3:
    // E110 00nn
    n = 0x03 & vib->vife[0];
    snprintf(buff, sizeof(buff), "Temperature Difference (%s degree F)", mbus_unit_prefix(n -3));
    break;
case 0x64:
case 0x64 + 1:
case 0x64 + 2:
case 0x64 + 3:
    // E110 01nn
    n = 0x03 & vib->vife[0];
    snprintf(buff, sizeof(buff), "External Temperature (%s degree F)", mbus_unit_prefix(n -3));
    break;
case 0x68:
case 0x69:
case 0x6A:
case 0x6B:
case 0x6C:
case 0x6D:
case 0x6E:
case 0x6F:
    // E110 1nnn
    snprintf(buff, sizeof(buff), "Reserved (0x%.2x)", vib->vife[0]);
    break;
case 0x70:
case 0x70 + 1:
case 0x70 + 2:
case 0x70 + 3:
    // E111 00nn
    n = 0x03 & vib->vife[0];
    snprintf(buff, sizeof(buff), "Cold / Warm Temperature Limit (%s degree F)", mbus_unit_prefix(n -3));
    break;
case 0x74:
case 0x74 + 1:
case 0x74 + 2:
case 0x74 + 3:
    // E111 00nn
    n = 0x03 & vib->vife[0];
    snprintf(buff, sizeof(buff), "Cold / Warm Temperature Limit (%s degree C)", mbus_unit_prefix(n -3));
    break;
case 0x78:
case 0x78 + 1:
case 0x78 + 2:
case 0x78 + 3:
case 0x78 + 4:
case 0x78 + 5:
case 0x78 + 6:
case 0x78 + 7:
    // E111 1nnn
    n = 0x07 & vib->vife[0];
    snprintf(buff, sizeof(buff), "cumul. count max power (%s W)", mbus_unit_prefix(n - 3));
    break;
default:
    snprintf(buff, sizeof(buff), "Unrecognized VIF 0xFB extension: 0x%.2x", vib->vife[0]);
    break;
}
return buff;

}
static const char *
mbus_vib_unit_lookup_fd(mbus_value_information_block *vib)
{
static char buff[256];
int n;
// ignore the extension bit in this selection
const unsigned char masked_vife0 = vib->vife[0] & MBUS_DIB_VIF_WITHOUT_EXTENSION;

if ((masked_vife0 & 0x7C) == 0x00)
{
    // VIFE = E000 00nn	Credit of 10nn-3 of the nominal local legal currency units
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Credit of %s of the nominal local legal currency units", mbus_unit_prefix(n - 3));
}
else if ((masked_vife0 & 0x7C) == 0x04)
{
    // VIFE = E000 01nn Debit of 10nn-3 of the nominal local legal currency units
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Debit of %s of the nominal local legal currency units", mbus_unit_prefix(n - 3));
}
else if (masked_vife0 == 0x08)
{
    // E000 1000
    snprintf(buff, sizeof(buff), "Access Number (transmission count)");
}
else if (masked_vife0 == 0x09)
{
    // E000 1001
    snprintf(buff, sizeof(buff), "Medium (as in fixed header)");
}
else if (masked_vife0 == 0x0A)
{
    // E000 1010
    snprintf(buff, sizeof(buff), "Manufacturer (as in fixed header)");
}
else if (masked_vife0 == 0x0B)
{
    // E000 1010
    snprintf(buff, sizeof(buff), "Parameter set identification");
}
else if (masked_vife0 == 0x0C)
{
    // E000 1100
    snprintf(buff, sizeof(buff), "Model / Version");
}
else if (masked_vife0 == 0x0D)
{
    // E000 1100
    snprintf(buff, sizeof(buff), "Hardware version");
}
else if (masked_vife0 == 0x0E)
{
    // E000 1101
    snprintf(buff, sizeof(buff), "Firmware version");
}
else if (masked_vife0 == 0x0F)
{
    // E000 1101
    snprintf(buff, sizeof(buff), "Software version");
}
else if (masked_vife0 == 0x10)
{
    // VIFE = E001 0000 Customer location
    snprintf(buff, sizeof(buff), "Customer location");
}
else if (masked_vife0 == 0x11)
{
    // VIFE = E001 0001 Customer
    snprintf(buff, sizeof(buff), "Customer");
}
else if (masked_vife0 == 0x12)
{
    // VIFE = E001 0010	Access Code User
    snprintf(buff, sizeof(buff), "Access Code User");
}
else if (masked_vife0 == 0x13)
{
    // VIFE = E001 0011	Access Code Operator
    snprintf(buff, sizeof(buff), "Access Code Operator");
}
else if (masked_vife0 == 0x14)
{
    // VIFE = E001 0100	Access Code System Operator
    snprintf(buff, sizeof(buff), "Access Code System Operator");
}
else if (masked_vife0 == 0x15)
{
    // VIFE = E001 0101	Access Code Developer
    snprintf(buff, sizeof(buff), "Access Code Developer");
}
else if (masked_vife0 == 0x16)
{
    // VIFE = E001 0110 Password
    snprintf(buff, sizeof(buff), "Password");
}
else if (masked_vife0 == 0x17)
{
    // VIFE = E001 0111 Error flags
    snprintf(buff, sizeof(buff), "Error flags");
}
else if (masked_vife0 == 0x18)
{
    // VIFE = E001 1000	Error mask
    snprintf(buff, sizeof(buff), "Error mask");
}
else if (masked_vife0 == 0x19)
{
    // VIFE = E001 1001	Reserved
    snprintf(buff, sizeof(buff), "Reserved");
}
else if (masked_vife0 == 0x1A)
{
    // VIFE = E001 1010 Digital output (binary)
    snprintf(buff, sizeof(buff), "Digital output (binary)");
}
else if (masked_vife0 == 0x1B)
{
    // VIFE = E001 1011 Digital input (binary)
    snprintf(buff, sizeof(buff), "Digital input (binary)");
}
else if (masked_vife0 == 0x1C)
{
    // VIFE = E001 1100	Baudrate [Baud]
    snprintf(buff, sizeof(buff), "Baudrate");
}
else if (masked_vife0 == 0x1D)
{
    // VIFE = E001 1101	response delay time [bittimes]
    snprintf(buff, sizeof(buff), "response delay time");
}
else if (masked_vife0 == 0x1E)
{
    // VIFE = E001 1110	Retry
    snprintf(buff, sizeof(buff), "Retry");
}
else if (masked_vife0 == 0x1F)
{
    // VIFE = E001 1111	Reserved
    snprintf(buff, sizeof(buff), "Reserved");
}
else if (masked_vife0 == 0x20)
{
    // VIFE = E010 0000	First storage # for cyclic storage
    snprintf(buff, sizeof(buff), "First storage # for cyclic storage");
}
else if (masked_vife0 == 0x21)
{
    // VIFE = E010 0001	Last storage # for cyclic storage
    snprintf(buff, sizeof(buff), "Last storage # for cyclic storage");
}
else if (masked_vife0 == 0x22)
{
    // VIFE = E010 0010	Size of storage block
    snprintf(buff, sizeof(buff), "Size of storage block");
}
else if (masked_vife0 == 0x23)
{
    // VIFE = E010 0011	Reserved
    snprintf(buff, sizeof(buff), "Reserved");
}
else if ((masked_vife0 & 0x7C) == 0x24)
{
    // VIFE = E010 01nn	Storage interval [sec(s)..day(s)]
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Storage interval %s", mbus_unit_duration_nn(n));
}
else if (masked_vife0 == 0x28)
{
    // VIFE = E010 1000	Storage interval month(s)
    snprintf(buff, sizeof(buff), "Storage interval month(s)");
}
else if (masked_vife0 == 0x29)
{
    // VIFE = E010 1001	Storage interval year(s)
    snprintf(buff, sizeof(buff), "Storage interval year(s)");
}
else if (masked_vife0 == 0x2A)
{
    // VIFE = E010 1010	Reserved
    snprintf(buff, sizeof(buff), "Reserved");
}
else if (masked_vife0 == 0x2B)
{
    // VIFE = E010 1011	Reserved
    snprintf(buff, sizeof(buff), "Reserved");
}
else if ((masked_vife0 & 0x7C) == 0x2C)
{
    // VIFE = E010 11nn	Duration since last readout [sec(s)..day(s)]
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Duration since last readout %s", mbus_unit_duration_nn(n));
}
else if (masked_vife0 == 0x30)
{
    // VIFE = E011 0000	Start (date/time) of tariff
    snprintf(buff, sizeof(buff), "Start (date/time) of tariff");
}
else if ((masked_vife0 & 0x7C) == 0x30)
{
    // VIFE = E011 00nn	Duration of tariff (nn=01 ..11: min to days)
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Duration of tariff %s", mbus_unit_duration_nn(n));
}
else if ((masked_vife0 & 0x7C) == 0x34)
{
    // VIFE = E011 01nn	Period of tariff [sec(s) to day(s)]
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Period of tariff %s", mbus_unit_duration_nn(n));
}
else if (masked_vife0 == 0x38)
{
    // VIFE = E011 1000	Period of tariff months(s)
    snprintf(buff, sizeof(buff), "Period of tariff months(s)");
}
else if (masked_vife0 == 0x39)
{
    // VIFE = E011 1001	Period of tariff year(s)
    snprintf(buff, sizeof(buff), "Period of tariff year(s)");
}
else if (masked_vife0 == 0x3A)
{
    // VIFE = E011 1010	dimensionless / no VIF
    snprintf(buff, sizeof(buff), "dimensionless / no VIF");
}
else if (masked_vife0 == 0x3B)
{
    // VIFE = E011 1011	Reserved
    snprintf(buff, sizeof(buff), "Reserved");
}
else if ((masked_vife0 & 0x7C) == 0x3C)
{
    // VIFE = E011 11xx	Reserved
    snprintf(buff, sizeof(buff), "Reserved");
}
else if ((masked_vife0 & 0x70) == 0x40)
{
    // VIFE = E100 nnnn 10^(nnnn-9) V
    n = (masked_vife0 & 0x0F);
    snprintf(buff, sizeof(buff), "%s V", mbus_unit_prefix(n - 9));
}
else if ((masked_vife0 & 0x70) == 0x50)
{
    // VIFE = E101 nnnn 10nnnn-12 A
    n = (masked_vife0 & 0x0F);
    snprintf(buff, sizeof(buff), "%s A", mbus_unit_prefix(n - 12));
}
else if (masked_vife0 == 0x60) {
    // VIFE = E110 0000	Reset counter
    snprintf(buff, sizeof(buff), "Reset counter");
}
else if (masked_vife0 == 0x61) {
    // VIFE = E110 0001	Cumulation counter
    snprintf(buff, sizeof(buff), "Cumulation counter");
}
else if (masked_vife0 == 0x62) {
    // VIFE = E110 0010	Control signal
    snprintf(buff, sizeof(buff), "Control signal");
}
else if (masked_vife0 == 0x63) {
    // VIFE = E110 0011	Day of week
    snprintf(buff, sizeof(buff), "Day of week");
}
else if (masked_vife0 == 0x64) {
    // VIFE = E110 0100	Week number
    snprintf(buff, sizeof(buff), "Week number");
}
else if (masked_vife0 == 0x65) {
    // VIFE = E110 0101	Time point of day change
    snprintf(buff, sizeof(buff), "Time point of day change");
}
else if (masked_vife0 == 0x66) {
    // VIFE = E110 0110	State of parameter activation
    snprintf(buff, sizeof(buff), "State of parameter activation");
}
else if (masked_vife0 == 0x67) {
    // VIFE = E110 0111	Special supplier information
    snprintf(buff, sizeof(buff), "Special supplier information");
}
else if ((masked_vife0 & 0x7C) == 0x68) {
    // VIFE = E110 10pp	Duration since last cumulation [hour(s)..years(s)]Ž
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Duration since last cumulation %s", mbus_unit_duration_pp(n));
}
else if ((masked_vife0 & 0x7C) == 0x6C) {
    // VIFE = E110 11pp	Operating time battery [hour(s)..years(s)]Ž
    n = (masked_vife0 & 0x03);
    snprintf(buff, sizeof(buff), "Operating time battery %s", mbus_unit_duration_pp(n));
}
else if (masked_vife0 == 0x70) {
    // VIFE = E111 0000	Date and time of battery change
    snprintf(buff, sizeof(buff), "Date and time of battery change");
}
else if ((masked_vife0 & 0x70) == 0x70)
{
    // VIFE = E111 nnn Reserved
    snprintf(buff, sizeof(buff), "Reserved VIF extension");
}
else
{
    snprintf(buff, sizeof(buff), "Unrecognized VIF 0xFD extension: 0x%.2x", masked_vife0);
}

return buff;

}
//------------------------------------------------------------------------------
/// Lookup the unit from the VIB (VIF or VIFE)
//
// Enhanced Identification
// E000 1000 Access Number (transmission count)
// E000 1001 Medium (as in fixed header)
// E000 1010 Manufacturer (as in fixed header)
// E000 1011 Parameter set identification
// E000 1100 Model / Version
// E000 1101 Hardware version #
// E000 1110 Firmware version #
// E000 1111 Software version #
//------------------------------------------------------------------------------
const char *
mbus_vib_unit_lookup(mbus_value_information_block *vib)
{
static char buff[256];
int n;
if (vib == NULL)
    return "";

if (vib->vif == 0xFB) // first type of VIF extention: see table 8.4.4
{
    if (vib->nvife == 0)
    {
        return "Missing VIF extension";
    }

    return mbus_vib_unit_lookup_fb(vib);
}
else if (vib->vif == 0xFD) // first type of VIF extention: see table 8.4.4
{
    if (vib->nvife == 0)
    {
        return "Missing VIF extension";
    }

    return mbus_vib_unit_lookup_fd(vib);
}
else if (vib->vif == 0x7C)
{
    // custom VIF
    snprintf(buff, sizeof(buff), "%s", vib->custom_vif);
    return buff;
}
else if (vib->vif == 0xFC && (vib->vife[0] & 0x78) == 0x70)
{
    // custom VIF
    n = (vib->vife[0] & 0x07);
    snprintf(buff, sizeof(buff), "%s %s", mbus_unit_prefix(n-6), vib->custom_vif);
    return buff;
}

return mbus_vif_unit_lookup(vib->vif); // no extention, use VIF

}
//------------------------------------------------------------------------------
// Decode data and write to string
//
// Data format (for record->data data array)
//
// Length in Bit Code Meaning Code Meaning
// 0 0000 No data 1000 Selection for Readout
// 8 0001 8 Bit Integer 1001 2 digit BCD
// 16 0010 16 Bit Integer 1010 4 digit BCD
// 24 0011 24 Bit Integer 1011 6 digit BCD
// 32 0100 32 Bit Integer 1100 8 digit BCD
// 32 / N 0101 32 Bit Real 1101 variable length
// 48 0110 48 Bit Integer 1110 12 digit BCD
// 64 0111 64 Bit Integer 1111 Special Functions
//
// The Code is stored in record->drh.dib.dif
//
///
/// Return a string containing the data
///
// Source: MBDOC48.PDF
//
//------------------------------------------------------------------------------
const char *
mbus_data_record_decode(mbus_data_record *record)
{
static char buff[768];
unsigned char vif, vife;
if (record)
{
    int int_val;
    float float_val;
    long long long_long_val;
    struct tm time;

    // ignore extension bit
    vif = (record->drh.vib.vif & MBUS_DIB_VIF_WITHOUT_EXTENSION);
    vife = (record->drh.vib.vife[0] & MBUS_DIB_VIF_WITHOUT_EXTENSION);

    switch (record->drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_DATA)
    {
        case 0x00: // no data

            buff[0] = 0;

            break;

        case 0x01: // 1 byte integer (8 bit)

            mbus_data_int_decode(record->data, 1, &int_val);

            snprintf(buff, sizeof(buff), "%d", int_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 1 byte integer\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;


        case 0x02: // 2 byte (16 bit)

            // E110 1100  Time Point (date)
            if (vif == 0x6C)
            {
                mbus_data_tm_decode(&time, record->data, 2);
                snprintf(buff, sizeof(buff), "%04d-%02d-%02d",
                                             (time.tm_year + 1900),
                                             (time.tm_mon + 1),
                                              time.tm_mday);
            }
            else  // 2 byte integer
            {
                mbus_data_int_decode(record->data, 2, &int_val);
                snprintf(buff, sizeof(buff), "%d", int_val);
                if (debug)
                    printf("%s: DIF 0x%.2x was decoded using 2 byte integer\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            }

            break;

        case 0x03: // 3 byte integer (24 bit)

            mbus_data_int_decode(record->data, 3, &int_val);

            snprintf(buff, sizeof(buff), "%d", int_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 3 byte integer\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x04: // 4 byte (32 bit)

            // E110 1101  Time Point (date/time)
            // E011 0000  Start (date/time) of tariff
            // E111 0000  Date and time of battery change
            if ( (vif == 0x6D) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x30)) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x70)))
            {
                mbus_data_tm_decode(&time, record->data, 4);
                snprintf(buff, sizeof(buff), "%04d-%02d-%02dT%02d:%02d:%02d",
                                             (time.tm_year + 1900),
                                             (time.tm_mon + 1),
                                              time.tm_mday,
                                              time.tm_hour,
                                              time.tm_min,
                                              time.tm_sec);
            }
            else  // 4 byte integer
            {
                mbus_data_int_decode(record->data, 4, &int_val);
                snprintf(buff, sizeof(buff), "%d", int_val);
            }

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 4 byte integer\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x05: // 4 Byte Real (32 bit)

            float_val = mbus_data_float_decode(record->data);

            snprintf(buff, sizeof(buff), "%f", float_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 4 byte Real\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x06: // 6 byte (48 bit)

            // E110 1101  Time Point (date/time)
            // E011 0000  Start (date/time) of tariff
            // E111 0000  Date and time of battery change
            if ( (vif == 0x6D) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x30)) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x70)))
            {
                mbus_data_tm_decode(&time, record->data, 6);
                snprintf(buff, sizeof(buff), "%04d-%02d-%02dT%02d:%02d:%02d",
                                             (time.tm_year + 1900),
                                             (time.tm_mon + 1),
                                              time.tm_mday,
                                              time.tm_hour,
                                              time.tm_min,
                                              time.tm_sec);
            }
            else  // 6 byte integer
            {
                mbus_data_long_long_decode(record->data, 6, &long_long_val);
                snprintf(buff, sizeof(buff), "%lld", long_long_val);
            }

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 6 byte integer\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x07: // 8 byte integer (64 bit)

            mbus_data_long_long_decode(record->data, 8, &long_long_val);

            snprintf(buff, sizeof(buff), "%lld", long_long_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 8 byte integer\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        //case 0x08:

        case 0x09: // 2 digit BCD (8 bit)

            int_val = (int)mbus_data_bcd_decode_hex(record->data, 1);
            snprintf(buff, sizeof(buff), "%X", int_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 2 digit BCD\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x0A: // 4 digit BCD (16 bit)

            int_val = (int)mbus_data_bcd_decode_hex(record->data, 2);
            snprintf(buff, sizeof(buff), "%X", int_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 4 digit BCD\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x0B: // 6 digit BCD (24 bit)

            int_val = (int)mbus_data_bcd_decode_hex(record->data, 3);
            snprintf(buff, sizeof(buff), "%X", int_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 6 digit BCD\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x0C: // 8 digit BCD (32 bit)

            int_val = (int)mbus_data_bcd_decode_hex(record->data, 4);
            snprintf(buff, sizeof(buff), "%X", int_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 8 digit BCD\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x0E: // 12 digit BCD (48 bit)

            long_long_val = mbus_data_bcd_decode_hex(record->data, 6);
            snprintf(buff, sizeof(buff), "%llX", long_long_val);

            if (debug)
                printf("%s: DIF 0x%.2x was decoded using 12 digit BCD\n", __PRETTY_FUNCTION__, record->drh.dib.dif);

            break;

        case 0x0F: // special functions

            mbus_data_bin_decode(buff, record->data, record->data_len, sizeof(buff));
            break;

        case 0x0D: // variable length
            if (record->data_len <= 0xBF)
            {
                mbus_data_str_decode(buff, record->data, record->data_len);
                break;
            }
            /*@fallthrough@*/

        default:

            snprintf(buff, sizeof(buff), "Unknown DIF (0x%.2x)", record->drh.dib.dif);
            break;
    }

    return buff;
}

return NULL;

}
//------------------------------------------------------------------------------
/// Return the unit description for a variable-length data record
//------------------------------------------------------------------------------
const char *
mbus_data_record_unit(mbus_data_record *record)
{
static char buff[128];
if (record)
{
    snprintf(buff, sizeof(buff), "%s", mbus_vib_unit_lookup(&(record->drh.vib)));

    return buff;
}

return NULL;

}
//------------------------------------------------------------------------------
/// Return the value for a variable-length data record
//------------------------------------------------------------------------------
const char *
mbus_data_record_value(mbus_data_record *record)
{
static char buff[768];
if (record)
{
    snprintf(buff, sizeof(buff), "%s", mbus_data_record_decode(record));

    return buff;
}

return NULL;

}
//------------------------------------------------------------------------------
/// Return the storage number for a variable-length data record
//------------------------------------------------------------------------------
long mbus_data_record_storage_number(mbus_data_record *record)
{
int bit_index = 0;
long result = 0;
int i;
if (record)
{
    result |= (record->drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO) >> 6;
    bit_index++;

    for (i=0; i<record->drh.dib.ndife; i++)
    {
        result |= (record->drh.dib.dife[i] & MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO) << bit_index;
        bit_index += 4;
    }

    return result;
}

return -1;

}
//------------------------------------------------------------------------------
/// Return the tariff for a variable-length data record
//------------------------------------------------------------------------------
long mbus_data_record_tariff(mbus_data_record *record)
{
int bit_index = 0;
long result = 0;
int i;
if (record && (record->drh.dib.ndife > 0))
{
    for (i=0; i<record->drh.dib.ndife; i++)
    {
        result |= ((record->drh.dib.dife[i] & MBUS_DATA_RECORD_DIFE_MASK_TARIFF) >> 4) << bit_index;
        bit_index += 2;
    }

    return result;
}

return -1;

}
//------------------------------------------------------------------------------
/// Return device unit for a variable-length data record
//------------------------------------------------------------------------------
int mbus_data_record_device(mbus_data_record *record)
{
int bit_index = 0;
int result = 0;
int i;
if (record && (record->drh.dib.ndife > 0))
{
    for (i=0; i<record->drh.dib.ndife; i++)
    {
        result |= ((record->drh.dib.dife[i] & MBUS_DATA_RECORD_DIFE_MASK_DEVICE) >> 6) << bit_index;
        bit_index++;
    }

    return result;
}

return -1;

}
//------------------------------------------------------------------------------
/// Return a string containing the function description
//------------------------------------------------------------------------------
const char *
mbus_data_record_function(mbus_data_record *record)
{
static char buff[128];
if (record)
{
    switch (record->drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_FUNCTION)
    {
        case 0x00:
            snprintf(buff, sizeof(buff), "Instantaneous value");
            break;

        case 0x10:
            snprintf(buff, sizeof(buff), "Maximum value");
            break;

        case 0x20:
            snprintf(buff, sizeof(buff), "Minimum value");
            break;

        case 0x30:
            snprintf(buff, sizeof(buff), "Value during error state");
            break;

        default:
            snprintf(buff, sizeof(buff), "unknown");
    }

    return buff;
}

return NULL;

}
///
/// For fixed-length frames, return a string describing the type of value (stored or actual)
///
const char *
mbus_data_fixed_function(int status)
{
static char buff[128];
snprintf(buff, sizeof(buff), "%s",
        (status & MBUS_DATA_FIXED_STATUS_DATE_MASK) == MBUS_DATA_FIXED_STATUS_DATE_STORED ?
        "Stored value" : "Actual value" );

return buff;

}
//------------------------------------------------------------------------------
//
// PARSER FUNCTIONS
//
//------------------------------------------------------------------------------
//------------------------------------------------------------------------------
/// PARSE M-BUS frame data structures from binary data.
//------------------------------------------------------------------------------
int
mbus_parse(mbus_frame *frame, unsigned char *data, size_t data_size)
{
size_t i, len;
if (frame && data && data_size > 0)
{
    frame->next = NULL;

    if (parse_debug)
        printf("%s: Attempting to parse binary data [size = %zu]\n", __PRETTY_FUNCTION__, data_size);

    if (parse_debug)
        printf("%s: ", __PRETTY_FUNCTION__);

    for (i = 0; i < data_size && parse_debug; i++)
    {
        printf("%.2X ", data[i] & 0xFF);
    }

    if (parse_debug)
        printf("\n%s: done.\n", __PRETTY_FUNCTION__);

    switch (data[0])
    {
        case MBUS_FRAME_ACK_START:

            // OK, got a valid ack frame, require no more data
            frame->start1   = data[0];
            frame->type = MBUS_FRAME_TYPE_ACK;
            return 0;
            //return MBUS_FRAME_BASE_SIZE_ACK - 1; // == 0

        case MBUS_FRAME_SHORT_START:

            if (data_size < MBUS_FRAME_BASE_SIZE_SHORT)
            {
                // OK, got a valid short packet start, but we need more data
                return MBUS_FRAME_BASE_SIZE_SHORT - data_size;
            }

            if (data_size != MBUS_FRAME_BASE_SIZE_SHORT)
            {
                snprintf(error_str, sizeof(error_str), "Too much data in frame.");

                // too much data... ?
                return -2;
            }

            // init frame data structure
            frame->start1   = data[0];
            frame->control  = data[1];
            frame->address  = data[2];
            frame->checksum = data[3];
            frame->stop     = data[4];

            frame->type = MBUS_FRAME_TYPE_SHORT;

            // verify the frame
            if (mbus_frame_verify(frame) != 0)
            {
                return -3;
            }

            // successfully parsed data
            return 0;

        case MBUS_FRAME_LONG_START: // (also CONTROL)

            if (data_size < 3)
            {
                // OK, got a valid long/control packet start, but we need
                // more data to determine the length
                return 3 - data_size;
            }

            // init frame data structure
            frame->start1   = data[0];
            frame->length1  = data[1];
            frame->length2  = data[2];

            if (frame->length1 < 3)
            {
                snprintf(error_str, sizeof(error_str), "Invalid M-Bus frame length.");

                // not a valid M-bus frame
                return -2;
            }

            if (frame->length1 != frame->length2)
            {
                snprintf(error_str, sizeof(error_str), "Invalid M-Bus frame length.");

                // not a valid M-bus frame
                return -2;
            }

            // check length of packet:
            len = frame->length1;

            if (data_size < (size_t)(MBUS_FRAME_FIXED_SIZE_LONG + len))
            {
                // OK, but we need more data
                return MBUS_FRAME_FIXED_SIZE_LONG + len - data_size;
            }

            if (data_size > (size_t)(MBUS_FRAME_FIXED_SIZE_LONG + len))
            {
                snprintf(error_str, sizeof(error_str), "Too much data in frame.");

                // too much data... ?
                return -2;
            }

            // we got the whole packet, continue parsing
            frame->start2   = data[3];
            frame->control  = data[4];
            frame->address  = data[5];
            frame->control_information = data[6];

            frame->data_size = len - 3;
            for (i = 0; i < frame->data_size; i++)
            {
                frame->data[i] = data[7 + i];
            }

            frame->checksum = data[data_size-2]; // data[6 + frame->data_size + 1]
            frame->stop     = data[data_size-1]; // data[6 + frame->data_size + 2]

            if (frame->data_size == 0)
            {
                frame->type = MBUS_FRAME_TYPE_CONTROL;
            }
            else
            {
                frame->type = MBUS_FRAME_TYPE_LONG;
            }

            // verify the frame
            if (mbus_frame_verify(frame) != 0)
            {
                return -3;
            }

            // successfully parsed data
            return 0;
        default:
            snprintf(error_str, sizeof(error_str), "Invalid M-Bus frame start.");

            // not a valid M-Bus frame header (start byte)
            return -4;
    }

}

snprintf(error_str, sizeof(error_str), "Got null pointer to frame, data or zero data_size.");

return -1;

}
//------------------------------------------------------------------------------
/// Parse the fixed-length data of a M-Bus frame
//------------------------------------------------------------------------------
int
mbus_data_fixed_parse(mbus_frame *frame, mbus_data_fixed *data)
{
if (frame && data)
{
if (frame->data_size != MBUS_DATA_FIXED_LENGTH)
{
snprintf(error_str, sizeof(error_str), "Invalid length for fixed data.");
return -1;
}
    // copy the fixed-length data structure bytewise
    data->id_bcd[0]   = frame->data[0];
    data->id_bcd[1]   = frame->data[1];
    data->id_bcd[2]   = frame->data[2];
    data->id_bcd[3]   = frame->data[3];
    data->tx_cnt      = frame->data[4];
    data->status      = frame->data[5];
    data->cnt1_type   = frame->data[6];
    data->cnt2_type   = frame->data[7];
    data->cnt1_val[0] = frame->data[8];
    data->cnt1_val[1] = frame->data[9];
    data->cnt1_val[2] = frame->data[10];
    data->cnt1_val[3] = frame->data[11];
    data->cnt2_val[0] = frame->data[12];
    data->cnt2_val[1] = frame->data[13];
    data->cnt2_val[2] = frame->data[14];
    data->cnt2_val[3] = frame->data[15];

    return 0;
}

return -1;

}
//------------------------------------------------------------------------------
/// Parse the variable-length data of a M-Bus frame
//------------------------------------------------------------------------------
int
mbus_data_variable_parse(mbus_frame *frame, mbus_data_variable *data)
{
mbus_data_record *record = NULL;
size_t i, j;
if (frame && data)
{
    // parse header
    data->nrecords = 0;
    data->more_records_follow = 0;
    i = MBUS_DATA_VARIABLE_HEADER_LENGTH;

    if(frame->data_size < i)
    {
        snprintf(error_str, sizeof(error_str), "Variable header too short.");
        return -1;
    }

    // first copy the variable data fixed header bytewise
    data->header.id_bcd[0]       = frame->data[0];
    data->header.id_bcd[1]       = frame->data[1];
    data->header.id_bcd[2]       = frame->data[2];
    data->header.id_bcd[3]       = frame->data[3];
    data->header.manufacturer[0] = frame->data[4];
    data->header.manufacturer[1] = frame->data[5];
    data->header.version         = frame->data[6];
    data->header.medium          = frame->data[7];
    data->header.access_no       = frame->data[8];
    data->header.status          = frame->data[9];
    data->header.signature[0]    = frame->data[10];
    data->header.signature[1]    = frame->data[11];

    data->record = NULL;

    while (i < frame->data_size)
    {
        // Skip filler dif=2F
        if ((frame->data[i] & 0xFF) == MBUS_DIB_DIF_IDLE_FILLER)
        {
          i++;
          continue;
        }

        if ((record = mbus_data_record_new()) == NULL)
        {
            // clean up...
            return (-2);
        }

        // copy timestamp
        memcpy((void *)&(record->timestamp), (void *)&(frame->timestamp), sizeof(time_t));

        // read and parse DIB (= DIF + DIFE)

        // DIF
        record->drh.dib.dif = frame->data[i];

        if ((record->drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC) ||
            (record->drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW))
        {
            if ((record->drh.dib.dif & 0xFF) == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW)
            {
              data->more_records_follow = 1;
            }

            i++;
            // just copy the remaining data as it is vendor specific
            record->data_len = frame->data_size - i;
            for (j = 0; j < record->data_len; j++)
            {
                record->data[j] = frame->data[i++];
            }

            // append the record and move on to next one
            mbus_data_record_append(data, record);
            data->nrecords++;
            continue;
        }

        // calculate length of data record
        record->data_len = mbus_dif_datalength_lookup(record->drh.dib.dif);

        // read DIF extensions
        record->drh.dib.ndife = 0;
        while ((i < frame->data_size) &&
               (frame->data[i] & MBUS_DIB_DIF_EXTENSION_BIT))
        {
            unsigned char dife;

            if (record->drh.dib.ndife >= MBUS_DATA_INFO_BLOCK_DIFE_SIZE)
            {
                mbus_data_record_free(record);
                snprintf(error_str, sizeof(error_str), "Too many DIFE.");
                return -1;
            }

            dife = frame->data[i+1];
            record->drh.dib.dife[record->drh.dib.ndife] = dife;

            record->drh.dib.ndife++;
            i++;
        }
        i++;

        if (i > frame->data_size)
        {
            mbus_data_record_free(record);
            snprintf(error_str, sizeof(error_str), "Premature end of record at DIF.");
            return -1;
        }

        // read and parse VIB (= VIF + VIFE)

        // VIF
        record->drh.vib.vif = frame->data[i++];

        if ((record->drh.vib.vif & MBUS_DIB_VIF_WITHOUT_EXTENSION) == 0x7C)
        {
            // variable length VIF in ASCII format
            int var_vif_len;
            var_vif_len = frame->data[i++];
            if (var_vif_len > MBUS_VALUE_INFO_BLOCK_CUSTOM_VIF_SIZE)
            {
                mbus_data_record_free(record);
                snprintf(error_str, sizeof(error_str), "Too long variable length VIF.");
                return -1;
            }

            if (i + var_vif_len > frame->data_size)
            {
                mbus_data_record_free(record);
                snprintf(error_str, sizeof(error_str), "Premature end of record at variable length VIF.");
                return -1;
            }
            mbus_data_str_decode(record->drh.vib.custom_vif, &(frame->data[i]), var_vif_len);
            i += var_vif_len;
        }

        // VIFE
        record->drh.vib.nvife = 0;

        if (record->drh.vib.vif & MBUS_DIB_VIF_EXTENSION_BIT)
        {
            record->drh.vib.vife[0] = frame->data[i];
            record->drh.vib.nvife++;

            while ((i < frame->data_size) &&
                   (frame->data[i] & MBUS_DIB_VIF_EXTENSION_BIT))
            {
                unsigned char vife;

                if (record->drh.vib.nvife >= MBUS_VALUE_INFO_BLOCK_VIFE_SIZE)
                {
                    mbus_data_record_free(record);
                    snprintf(error_str, sizeof(error_str), "Too many VIFE.");
                    return -1;
                }

                vife = frame->data[i+1];
                record->drh.vib.vife[record->drh.vib.nvife] = vife;

                record->drh.vib.nvife++;
                i++;
            }
            i++;
        }

        if (i > frame->data_size)
        {
            mbus_data_record_free(record);
            snprintf(error_str, sizeof(error_str), "Premature end of record at VIF.");
            return -1;
        }

        // re-calculate data length, if of variable length type
        if ((record->drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_DATA) == 0x0D) // flag for variable length data
        {
            if(frame->data[i] <= 0xBF)
                record->data_len = frame->data[i++];
            else if(frame->data[i] >= 0xC0 && frame->data[i] <= 0xCF)
                record->data_len = (frame->data[i++] - 0xC0) * 2;
            else if(frame->data[i] >= 0xD0 && frame->data[i] <= 0xDF)
                record->data_len = (frame->data[i++] - 0xD0) * 2;
            else if(frame->data[i] >= 0xE0 && frame->data[i] <= 0xEF)
                record->data_len = frame->data[i++] - 0xE0;
            else if(frame->data[i] >= 0xF0 && frame->data[i] <= 0xFA)
                record->data_len = frame->data[i++] - 0xF0;
        }

        if (i + record->data_len > frame->data_size)
        {
            mbus_data_record_free(record);
            snprintf(error_str, sizeof(error_str), "Premature end of record at data.");
            return -1;
        }

        // copy data
        for (j = 0; j < record->data_len; j++)
        {
            record->data[j] = frame->data[i++];
        }

        // append the record and move on to next one
        mbus_data_record_append(data, record);
        data->nrecords++;
    }

    return 0;
}

return -1;

}
//------------------------------------------------------------------------------
/// Check the stype of the frame data (error, fixed or variable) and dispatch to the
/// corresponding parser function.
//------------------------------------------------------------------------------
int
mbus_frame_data_parse(mbus_frame *frame, mbus_frame_data *data)
{
char direction;
if (frame == NULL)
{
    snprintf(error_str, sizeof(error_str), "Got null pointer to frame.");
    return -1;
}

if (data == NULL)
{
    snprintf(error_str, sizeof(error_str), "Got null pointer to data.");
    return -1;
}

direction = (frame->control & MBUS_CONTROL_MASK_DIR);

if (direction == MBUS_CONTROL_MASK_DIR_S2M)
{
    if (frame->control_information == MBUS_CONTROL_INFO_ERROR_GENERAL)
    {
        data->type = MBUS_DATA_TYPE_ERROR;

        if (frame->data_size > 0)
        {
            data->error = (int) frame->data[0];
        }
        else
        {
            data->error = 0;
        }

        return 0;
    }
    else if (frame->control_information == MBUS_CONTROL_INFO_RESP_FIXED)
    {
        if (frame->data_size == 0)
        {
            snprintf(error_str, sizeof(error_str), "Got zero data_size.");

            return -1;
        }

        data->type = MBUS_DATA_TYPE_FIXED;
        return mbus_data_fixed_parse(frame, &(data->data_fix));
    }
    else if (frame->control_information == MBUS_CONTROL_INFO_RESP_VARIABLE)
    {
        if (frame->data_size == 0)
        {
            snprintf(error_str, sizeof(error_str), "Got zero data_size.");

            return -1;
        }

        data->type = MBUS_DATA_TYPE_VARIABLE;
        return mbus_data_variable_parse(frame, &(data->data_var));
    }
    else
    {
        snprintf(error_str, sizeof(error_str), "Unknown control information 0x%.2x", frame->control_information);

        return -1;
    }
}

snprintf(error_str, sizeof(error_str), "Wrong direction in frame (master to slave)");

return -1;

}
//------------------------------------------------------------------------------
/// Pack the M-bus frame into a binary string representation that can be sent
/// on the bus. The binary packet format is different for the different types
/// of M-bus frames.
//------------------------------------------------------------------------------
int
mbus_frame_pack(mbus_frame *frame, unsigned char *data, size_t data_size)
{
size_t i, offset = 0;
if (frame && data)
{
    if (mbus_frame_calc_length(frame) == -1)
    {
        return -2;
    }

    if (mbus_frame_calc_checksum(frame) == -1)
    {
        return -3;
    }

    switch (frame->type)
    {
        case MBUS_FRAME_TYPE_ACK:

            if (data_size < MBUS_FRAME_ACK_BASE_SIZE)
            {
                return -4;
            }

            data[offset++] = frame->start1;

            return offset;

        case MBUS_FRAME_TYPE_SHORT:

            if (data_size < MBUS_FRAME_SHORT_BASE_SIZE)
            {
                return -4;
            }

            data[offset++] = frame->start1;
            data[offset++] = frame->control;
            data[offset++] = frame->address;
            data[offset++] = frame->checksum;
            data[offset++] = frame->stop;

            return offset;

        case MBUS_FRAME_TYPE_CONTROL:

            if (data_size < MBUS_FRAME_CONTROL_BASE_SIZE)
            {
                return -4;
            }

            data[offset++] = frame->start1;
            data[offset++] = frame->length1;
            data[offset++] = frame->length2;
            data[offset++] = frame->start2;

            data[offset++] = frame->control;
            data[offset++] = frame->address;
            data[offset++] = frame->control_information;

            data[offset++] = frame->checksum;
            data[offset++] = frame->stop;

            return offset;

        case MBUS_FRAME_TYPE_LONG:

            if (data_size < frame->data_size + MBUS_FRAME_LONG_BASE_SIZE)
            {
                return -4;
            }

            data[offset++] = frame->start1;
            data[offset++] = frame->length1;
            data[offset++] = frame->length2;
            data[offset++] = frame->start2;

            data[offset++] = frame->control;
            data[offset++] = frame->address;
            data[offset++] = frame->control_information;

            for (i = 0; i < frame->data_size; i++)
            {
                data[offset++] = frame->data[i];
            }

            data[offset++] = frame->checksum;
            data[offset++] = frame->stop;

            return offset;

        default:
            return -5;
    }
}

return -1;

}
//------------------------------------------------------------------------------
/// pack the data stuctures into frame->data
//------------------------------------------------------------------------------
int
mbus_frame_internal_pack(mbus_frame *frame, mbus_frame_data *frame_data)
{
mbus_data_record *record;
int j;
if (frame == NULL || frame_data == NULL)
    return -1;

frame->data_size = 0;

switch (frame_data->type)
{
    case MBUS_DATA_TYPE_ERROR:

        frame->data[frame->data_size++] = (char) frame_data->error;

        break;

    case MBUS_DATA_TYPE_FIXED:

        //
        // pack fixed data structure
        //
        frame->data[frame->data_size++] = frame_data->data_fix.id_bcd[0];
        frame->data[frame->data_size++] = frame_data->data_fix.id_bcd[1];
        frame->data[frame->data_size++] = frame_data->data_fix.id_bcd[2];
        frame->data[frame->data_size++] = frame_data->data_fix.id_bcd[3];
        frame->data[frame->data_size++] = frame_data->data_fix.tx_cnt;
        frame->data[frame->data_size++] = frame_data->data_fix.status;
        frame->data[frame->data_size++] = frame_data->data_fix.cnt1_type;
        frame->data[frame->data_size++] = frame_data->data_fix.cnt2_type;
        frame->data[frame->data_size++] = frame_data->data_fix.cnt1_val[0];
        frame->data[frame->data_size++] = frame_data->data_fix.cnt1_val[1];
        frame->data[frame->data_size++] = frame_data->data_fix.cnt1_val[2];
        frame->data[frame->data_size++] = frame_data->data_fix.cnt1_val[3];
        frame->data[frame->data_size++] = frame_data->data_fix.cnt2_val[0];
        frame->data[frame->data_size++] = frame_data->data_fix.cnt2_val[1];
        frame->data[frame->data_size++] = frame_data->data_fix.cnt2_val[2];
        frame->data[frame->data_size++] = frame_data->data_fix.cnt2_val[3];

        break;

    case MBUS_DATA_TYPE_VARIABLE:

        //
        // first pack variable data structure header
        //
        frame->data[frame->data_size++] = frame_data->data_var.header.id_bcd[0];
        frame->data[frame->data_size++] = frame_data->data_var.header.id_bcd[1];
        frame->data[frame->data_size++] = frame_data->data_var.header.id_bcd[2];
        frame->data[frame->data_size++] = frame_data->data_var.header.id_bcd[3];
        frame->data[frame->data_size++] = frame_data->data_var.header.manufacturer[0];
        frame->data[frame->data_size++] = frame_data->data_var.header.manufacturer[1];
        frame->data[frame->data_size++] = frame_data->data_var.header.version;
        frame->data[frame->data_size++] = frame_data->data_var.header.medium;
        frame->data[frame->data_size++] = frame_data->data_var.header.access_no;
        frame->data[frame->data_size++] = frame_data->data_var.header.status;
        frame->data[frame->data_size++] = frame_data->data_var.header.signature[0];
        frame->data[frame->data_size++] = frame_data->data_var.header.signature[1];

        //
        // pack all data records
        //
        for (record = frame_data->data_var.record; record; record = record->next)
        {
            // pack DIF
            if (parse_debug)
                printf("%s: packing DIF [%zu]", __PRETTY_FUNCTION__, frame->data_size);
            frame->data[frame->data_size++] = record->drh.dib.dif;
            for (j = 0; j < record->drh.dib.ndife; j++)
            {
                frame->data[frame->data_size++] = record->drh.dib.dife[j];
            }

            // pack VIF
            if (parse_debug)
                printf("%s: packing VIF [%zu]", __PRETTY_FUNCTION__, frame->data_size);
            frame->data[frame->data_size++] = record->drh.vib.vif;
            for (j = 0; j < record->drh.vib.nvife; j++)
            {
                frame->data[frame->data_size++] = record->drh.vib.vife[j];
            }

            // pack data
            if (parse_debug)
                printf("%s: packing data [%zu : %zu]", __PRETTY_FUNCTION__, frame->data_size, record->data_len);
            for (j = 0; j < record->data_len; j++)
            {
                frame->data[frame->data_size++] = record->data[j];
            }
        }

        break;

    default:
        return -2;
}

return 0;

}
//------------------------------------------------------------------------------
//
// Print/Dump functions
//
//------------------------------------------------------------------------------
//------------------------------------------------------------------------------
/// Switch parse debugging
//------------------------------------------------------------------------------
void
mbus_parse_set_debug(int debug)
{
parse_debug = debug;
}
//------------------------------------------------------------------------------
/// Dump frame in HEX on standard output
//------------------------------------------------------------------------------
int
mbus_frame_print(mbus_frame *frame)
{
mbus_frame *iter;
unsigned char data_buff[256];
int len, i;
if (frame == NULL)
    return -1;

for (iter = frame; iter; iter = iter->next)
{
    if ((len = mbus_frame_pack(iter, data_buff, sizeof(data_buff))) == -1)
    {
        return -2;
    }

    printf("%s: Dumping M-Bus frame [type %d, %d bytes]: ", __PRETTY_FUNCTION__, iter->type, len);
    for (i = 0; i < len; i++)
    {
        printf("%.2X ", data_buff[i]);
    }
    printf("\n");
}

return 0;

}
//------------------------------------------------------------------------------
///
/// Print the data part of a frame.
///
//------------------------------------------------------------------------------
int
mbus_frame_data_print(mbus_frame_data *data)
{
if (data)
{
if (data->type == MBUS_DATA_TYPE_ERROR)
{
return mbus_data_error_print(data->error);
}
    if (data->type == MBUS_DATA_TYPE_FIXED)
    {
        return mbus_data_fixed_print(&(data->data_fix));
    }

    if (data->type == MBUS_DATA_TYPE_VARIABLE)
    {
        return mbus_data_variable_print(&(data->data_var));
    }
}

return -1;

}
//------------------------------------------------------------------------------
/// Print M-bus frame info to stdout
//------------------------------------------------------------------------------
int
mbus_data_variable_header_print(mbus_data_variable_header *header)
{
if (header)
{
printf("%s: ID = %llX\n", PRETTY_FUNCTION,
mbus_data_bcd_decode_hex(header->id_bcd, 4));
    printf("%s: Manufacturer = 0x%.2X%.2X\n", __PRETTY_FUNCTION__,
           header->manufacturer[1], header->manufacturer[0]);

    printf("%s: Manufacturer = %s\n", __PRETTY_FUNCTION__,
           mbus_decode_manufacturer(header->manufacturer[0], header->manufacturer[1]));

    printf("%s: Version      = 0x%.2X\n", __PRETTY_FUNCTION__, header->version);
    printf("%s: Medium       = %s (0x%.2X)\n", __PRETTY_FUNCTION__, mbus_data_variable_medium_lookup(header->medium), header->medium);
    printf("%s: Access #     = 0x%.2X\n", __PRETTY_FUNCTION__, header->access_no);
    printf("%s: Status       = 0x%.2X\n", __PRETTY_FUNCTION__, header->status);
    printf("%s: Signature    = 0x%.2X%.2X\n", __PRETTY_FUNCTION__,
           header->signature[1], header->signature[0]);

}

return -1;

}
int
mbus_data_variable_print(mbus_data_variable *data)
{
mbus_data_record *record;
size_t j;
int i;
if (data)
{
    mbus_data_variable_header_print(&(data->header));

    for (record = data->record, i = 0; record; record = record->next, i++)
    {
        printf("Record ID         = %d\n", i);
        // DIF
        printf("DIF               = %.2X\n", record->drh.dib.dif);
        printf("DIF.Extension     = %s\n",  (record->drh.dib.dif & MBUS_DIB_DIF_EXTENSION_BIT) ? "Yes":"No");
        printf("DIF.StorageNumber = %d\n",  (record->drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_STORAGE_NO) >> 6);
        printf("DIF.Function      = %s\n",  (record->drh.dib.dif & 0x30) ? "Value during error state" : "Instantaneous value" );
        printf("DIF.Data          = %.2X\n", record->drh.dib.dif & 0x0F);

        // VENDOR SPECIFIC
        if ((record->drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC) ||
            (record->drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW)) //MBUS_DIB_DIF_VENDOR_SPECIFIC
        {
            printf("%s: VENDOR DATA [size=%zu] = ", __PRETTY_FUNCTION__, record->data_len);
            for (j = 0; j < record->data_len; j++)
            {
                printf("%.2X ", record->data[j]);
            }
            printf("\n");

            if (record->drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW)
            {
              printf("%s: More records follow in next telegram\n", __PRETTY_FUNCTION__);
            }
            continue;
        }

        // calculate length of data record
        printf("DATA LENGTH = %zu\n", record->data_len);

        // DIFE
        for (j = 0; j < record->drh.dib.ndife; j++)
        {
            unsigned char dife = record->drh.dib.dife[j];

            printf("DIFE[%zu]               = %.2X\n", j,  dife);
            printf("DIFE[%zu].Extension     = %s\n",   j, (dife & MBUS_DATA_RECORD_DIFE_MASK_EXTENSION) ? "Yes" : "No");
            printf("DIFE[%zu].Device        = %d\n",   j, (dife & MBUS_DATA_RECORD_DIFE_MASK_DEVICE) >> 6 );
            printf("DIFE[%zu].Tariff        = %d\n",   j, (dife & MBUS_DATA_RECORD_DIFE_MASK_TARIFF) >> 4 );
            printf("DIFE[%zu].StorageNumber = %.2X\n", j,  dife & MBUS_DATA_RECORD_DIFE_MASK_STORAGE_NO);
        }

        // VIF
        printf("VIF           = %.2X\n", record->drh.vib.vif);
        printf("VIF.Extension = %s\n",  (record->drh.vib.vif & MBUS_DIB_VIF_EXTENSION_BIT) ? "Yes":"No");
        printf("VIF.Value     = %.2X\n", record->drh.vib.vif & MBUS_DIB_VIF_WITHOUT_EXTENSION);

        // VIFE
        for (j = 0; j < record->drh.vib.nvife; j++)
        {
            unsigned char vife = record->drh.vib.vife[j];

            printf("VIFE[%zu]           = %.2X\n", j,  vife);
            printf("VIFE[%zu].Extension = %s\n",   j, (vife & MBUS_DIB_VIF_EXTENSION_BIT) ? "Yes" : "No");
            printf("VIFE[%zu].Value     = %.2X\n", j,  vife & MBUS_DIB_VIF_WITHOUT_EXTENSION);
        }

        printf("\n");
    }
}

return -1;

}
int
mbus_data_fixed_print(mbus_data_fixed *data)
{
int val;
if (data)
{
    printf("%s: ID       = %llX\n", __PRETTY_FUNCTION__, mbus_data_bcd_decode_hex(data->id_bcd, 4));
    printf("%s: Access # = 0x%.2X\n", __PRETTY_FUNCTION__, data->tx_cnt);
    printf("%s: Status   = 0x%.2X\n", __PRETTY_FUNCTION__, data->status);
    printf("%s: Function = %s\n", __PRETTY_FUNCTION__, mbus_data_fixed_function(data->status));

    printf("%s: Medium1  = %s\n", __PRETTY_FUNCTION__, mbus_data_fixed_medium(data));
    printf("%s: Unit1    = %s\n", __PRETTY_FUNCTION__, mbus_data_fixed_unit(data->cnt1_type));
    if ((data->status & MBUS_DATA_FIXED_STATUS_FORMAT_MASK) == MBUS_DATA_FIXED_STATUS_FORMAT_BCD)
    {
        printf("%s: Counter1 = %llX\n", __PRETTY_FUNCTION__, mbus_data_bcd_decode_hex(data->cnt1_val, 4));
    }
    else
    {
        mbus_data_int_decode(data->cnt1_val, 4, &val);
        printf("%s: Counter1 = %d\n", __PRETTY_FUNCTION__, val);
    }

    printf("%s: Medium2  = %s\n", __PRETTY_FUNCTION__, mbus_data_fixed_medium(data));
    printf("%s: Unit2    = %s\n", __PRETTY_FUNCTION__, mbus_data_fixed_unit(data->cnt2_type));
    if ((data->status & MBUS_DATA_FIXED_STATUS_FORMAT_MASK) == MBUS_DATA_FIXED_STATUS_FORMAT_BCD)
    {
        printf("%s: Counter2 = %llX\n", __PRETTY_FUNCTION__, mbus_data_bcd_decode_hex(data->cnt2_val, 4));
    }
    else
    {
        mbus_data_int_decode(data->cnt2_val, 4, &val);
        printf("%s: Counter2 = %d\n", __PRETTY_FUNCTION__, val);
    }
}

return -1;

}
void
mbus_hex_dump(const char *label, const char *buff, size_t len)
{
time_t rawtime;
struct tm * timeinfo;
char timestamp
22
;
size_t i;
if (label == NULL || buff == NULL)
    return;

time ( &rawtime );
timeinfo = gmtime ( &rawtime );

strftime(timestamp,21,"%Y-%m-%d %H:%M:%SZ",timeinfo);
fprintf(stderr, "[%s] %s (%03zu):", timestamp, label, len);

for (i = 0; i < len; i++)
{
   fprintf(stderr, " %02X", (unsigned char) buff[i]);
}

fprintf(stderr, "\n");

}
int
mbus_data_error_print(int error)
{
printf("%s: Error = %d\n", PRETTY_FUNCTION, error);
return -1;

}
//------------------------------------------------------------------------------
//
// XML RELATED FUNCTIONS
//
//------------------------------------------------------------------------------
//------------------------------------------------------------------------------
///
/// Encode string to XML
///
//------------------------------------------------------------------------------
int
mbus_str_xml_encode(unsigned char *dst, const unsigned char *src, size_t max_len)
{
size_t i, len;
i = 0;
len = 0;

if (dst == NULL)
{
    return -1;
}

if (src == NULL)
{
    dst[len] = '\0';
    return -2;
}

while((len+6) < max_len)
{
    if (src[i] == '\0')
    {
        break;
    }

    if (iscntrl(src[i]))
    {
        // convert all control chars into spaces
        dst[len++] = ' ';
    }
    else
    {
        switch (src[i])
        {
            case '&':
                len += snprintf(&dst[len], max_len - len, "&amp;");
                break;
            case '<':
                len += snprintf(&dst[len], max_len - len, "&lt;");
                break;
            case '>':
                len += snprintf(&dst[len], max_len - len, "&gt;");
                break;
            case '"':
                len += snprintf(&dst[len], max_len - len, "&quot;");
                break;
            default:
                dst[len++] = src[i];
                break;
        }
    }

    i++;
}

dst[len] = '\0';
return 0;

}
//------------------------------------------------------------------------------
/// Generate XML for the variable-length data header
//------------------------------------------------------------------------------
char *
mbus_data_variable_header_xml(mbus_data_variable_header *header)
{
static char buff[8192];
char str_encoded[768];
size_t len = 0;
if (header)
{
    len += snprintf(&buff[len], sizeof(buff) - len, "    <SlaveInformation>\n");

    len += snprintf(&buff[len], sizeof(buff) - len, "        <Id>%llX</Id>\n", mbus_data_bcd_decode_hex(header->id_bcd, 4));
    len += snprintf(&buff[len], sizeof(buff) - len, "        <Manufacturer>%s</Manufacturer>\n",
            mbus_decode_manufacturer(header->manufacturer[0], header->manufacturer[1]));
    len += snprintf(&buff[len], sizeof(buff) - len, "        <Version>%d</Version>\n", header->version);

    mbus_str_xml_encode(str_encoded, mbus_data_product_name(header), sizeof(str_encoded));

    len += snprintf(&buff[len], sizeof(buff) - len, "        <ProductName>%s</ProductName>\n", str_encoded);

    mbus_str_xml_encode(str_encoded, mbus_data_variable_medium_lookup(header->medium), sizeof(str_encoded));

    len += snprintf(&buff[len], sizeof(buff) - len, "        <Medium>%s</Medium>\n", str_encoded);
    len += snprintf(&buff[len], sizeof(buff) - len, "        <AccessNumber>%d</AccessNumber>\n", header->access_no);
    len += snprintf(&buff[len], sizeof(buff) - len, "        <Status>%.2X</Status>\n", header->status);
    len += snprintf(&buff[len], sizeof(buff) - len, "        <Signature>%.2X%.2X</Signature>\n", header->signature[1], header->signature[0]);

    len += snprintf(&buff[len], sizeof(buff) - len, "    </SlaveInformation>\n\n");

    return buff;
}

return "";

}
//------------------------------------------------------------------------------
/// Generate XML for a single variable-length data record
//------------------------------------------------------------------------------
char *
mbus_data_variable_record_xml(mbus_data_record *record, int record_cnt, int frame_cnt, mbus_data_variable_header *header)
{
static char buff[8192];
char str_encoded[768];
size_t len = 0;
struct tm * timeinfo;
char timestamp
22
;
long tariff;
if (record)
{
    if (frame_cnt >= 0)
    {
        len += snprintf(&buff[len], sizeof(buff) - len,
                        "    <DataRecord id=\"%d\" frame=\"%d\">\n",
                        record_cnt, frame_cnt);
    }
    else
    {
        len += snprintf(&buff[len], sizeof(buff) - len,
                        "    <DataRecord id=\"%d\">\n", record_cnt);
    }

    if (record->drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC) // MBUS_DIB_DIF_VENDOR_SPECIFIC
    {
        len += snprintf(&buff[len], sizeof(buff) - len,
                        "        <Function>Manufacturer specific</Function>\n");
    }
    else if (record->drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW)
    {
        len += snprintf(&buff[len], sizeof(buff) - len,
                        "        <Function>More records follow</Function>\n");
    }
    else
    {
        mbus_str_xml_encode(str_encoded, mbus_data_record_function(record), sizeof(str_encoded));
        len += snprintf(&buff[len], sizeof(buff) - len,
                        "        <Function>%s</Function>\n", str_encoded);

        len += snprintf(&buff[len], sizeof(buff) - len,
                        "        <StorageNumber>%ld</StorageNumber>\n",
                        mbus_data_record_storage_number(record));

        if ((tariff = mbus_data_record_tariff(record)) >= 0)
        {
            len += snprintf(&buff[len], sizeof(buff) - len, "        <Tariff>%ld</Tariff>\n",
                            tariff);
            len += snprintf(&buff[len], sizeof(buff) - len, "        <Device>%d</Device>\n",
                            mbus_data_record_device(record));
        }

        mbus_str_xml_encode(str_encoded, mbus_data_record_unit(record), sizeof(str_encoded));
        len += snprintf(&buff[len], sizeof(buff) - len,
                        "        <Unit>%s</Unit>\n", str_encoded);
    }

    mbus_str_xml_encode(str_encoded, mbus_data_record_value(record), sizeof(str_encoded));
    len += snprintf(&buff[len], sizeof(buff) - len, "        <Value>%s</Value>\n", str_encoded);

    if (record->timestamp > 0)
    {
        timeinfo = gmtime (&(record->timestamp));
        strftime(timestamp,21,"%Y-%m-%dT%H:%M:%SZ",timeinfo);
        len += snprintf(&buff[len], sizeof(buff) - len,
                        "        <Timestamp>%s</Timestamp>\n", timestamp);
    }

    len += snprintf(&buff[len], sizeof(buff) - len, "    </DataRecord>\n\n");

    return buff;
}

return "";

}
//------------------------------------------------------------------------------
/// Generate XML for variable-length data
//------------------------------------------------------------------------------
char *
mbus_data_variable_xml(mbus_data_variable *data)
{
mbus_data_record *record;
char *buff = NULL, *new_buff;
size_t len = 0, buff_size = 8192;
int i;
if (data)
{
    buff = (char*) malloc(buff_size);

    if (buff == NULL)
        return NULL;

    len += snprintf(&buff[len], buff_size - len, MBUS_XML_PROCESSING_INSTRUCTION);

    len += snprintf(&buff[len], buff_size - len, "<MBusData>\n\n");

    len += snprintf(&buff[len], buff_size - len, "%s",
                    mbus_data_variable_header_xml(&(data->header)));

    for (record = data->record, i = 0; record; record = record->next, i++)
    {
        if ((buff_size - len) < 1024)
        {
            buff_size *= 2;
            new_buff = (char*) realloc(buff,buff_size);

            if (new_buff == NULL)
            {
                free(buff);
                return NULL;
            }

            buff = new_buff;
        }

        len += snprintf(&buff[len], buff_size - len, "%s",
                        mbus_data_variable_record_xml(record, i, -1, &(data->header)));
    }
    len += snprintf(&buff[len], buff_size - len, "</MBusData>\n");

    return buff;
}

return NULL;

}
//------------------------------------------------------------------------------
/// Generate XML representation of fixed-length frame.
//------------------------------------------------------------------------------
char *
mbus_data_fixed_xml(mbus_data_fixed *data)
{
char *buff = NULL;
char str_encoded[256];
size_t len = 0, buff_size = 8192;
int val;
if (data)
{
    buff = (char*) malloc(buff_size);

    if (buff == NULL)
        return NULL;

    len += snprintf(&buff[len], buff_size - len, MBUS_XML_PROCESSING_INSTRUCTION);

    len += snprintf(&buff[len], buff_size - len, "<MBusData>\n\n");

    len += snprintf(&buff[len], buff_size - len, "    <SlaveInformation>\n");
    len += snprintf(&buff[len], buff_size - len, "        <Id>%llX</Id>\n", mbus_data_bcd_decode_hex(data->id_bcd, 4));

    mbus_str_xml_encode(str_encoded, mbus_data_fixed_medium(data), sizeof(str_encoded));
    len += snprintf(&buff[len], buff_size - len, "        <Medium>%s</Medium>\n", str_encoded);

    len += snprintf(&buff[len], buff_size - len, "        <AccessNumber>%d</AccessNumber>\n", data->tx_cnt);
    len += snprintf(&buff[len], buff_size - len, "        <Status>%.2X</Status>\n", data->status);
    len += snprintf(&buff[len], buff_size - len, "    </SlaveInformation>\n\n");

    len += snprintf(&buff[len], buff_size - len, "    <DataRecord id=\"0\">\n");

    mbus_str_xml_encode(str_encoded, mbus_data_fixed_function(data->status), sizeof(str_encoded));
    len += snprintf(&buff[len], buff_size - len, "        <Function>%s</Function>\n", str_encoded);

    mbus_str_xml_encode(str_encoded, mbus_data_fixed_unit(data->cnt1_type), sizeof(str_encoded));
    len += snprintf(&buff[len], buff_size - len, "        <Unit>%s</Unit>\n", str_encoded);
    if ((data->status & MBUS_DATA_FIXED_STATUS_FORMAT_MASK) == MBUS_DATA_FIXED_STATUS_FORMAT_BCD)
    {
        len += snprintf(&buff[len], buff_size - len, "        <Value>%llX</Value>\n", mbus_data_bcd_decode_hex(data->cnt1_val, 4));
    }
    else
    {
        mbus_data_int_decode(data->cnt1_val, 4, &val);
        len += snprintf(&buff[len], buff_size - len, "        <Value>%d</Value>\n", val);
    }

    len += snprintf(&buff[len], buff_size - len, "    </DataRecord>\n\n");

    len += snprintf(&buff[len], buff_size - len, "    <DataRecord id=\"1\">\n");

    mbus_str_xml_encode(str_encoded, mbus_data_fixed_function(data->status), sizeof(str_encoded));
    len += snprintf(&buff[len], buff_size - len, "        <Function>%s</Function>\n", str_encoded);

    mbus_str_xml_encode(str_encoded, mbus_data_fixed_unit(data->cnt2_type), sizeof(str_encoded));
    len += snprintf(&buff[len], buff_size - len, "        <Unit>%s</Unit>\n", str_encoded);
    if ((data->status & MBUS_DATA_FIXED_STATUS_FORMAT_MASK) == MBUS_DATA_FIXED_STATUS_FORMAT_BCD)
    {
        len += snprintf(&buff[len], buff_size - len, "        <Value>%llX</Value>\n", mbus_data_bcd_decode_hex(data->cnt2_val, 4));
    }
    else
    {
        mbus_data_int_decode(data->cnt2_val, 4, &val);
        len += snprintf(&buff[len], buff_size - len, "        <Value>%d</Value>\n", val);
    }

    len += snprintf(&buff[len], buff_size - len, "    </DataRecord>\n\n");

    len += snprintf(&buff[len], buff_size - len, "</MBusData>\n");

    return buff;
}

return NULL;

}
//------------------------------------------------------------------------------
/// Generate XML representation of a general application error.
//------------------------------------------------------------------------------
char *
mbus_data_error_xml(int error)
{
char *buff = NULL;
char str_encoded[256];
size_t len = 0, buff_size = 8192;
buff = (char*) malloc(buff_size);

if (buff == NULL)
    return NULL;

len += snprintf(&buff[len], buff_size - len, MBUS_XML_PROCESSING_INSTRUCTION);
len += snprintf(&buff[len], buff_size - len, "<MBusData>\n\n");

len += snprintf(&buff[len], buff_size - len, "    <SlaveInformation>\n");

mbus_str_xml_encode(str_encoded, mbus_data_error_lookup(error), sizeof(str_encoded));
len += snprintf(&buff[len], buff_size - len, "        <Error>%s</Error>\n", str_encoded);

len += snprintf(&buff[len], buff_size - len, "    </SlaveInformation>\n\n");

len += snprintf(&buff[len], buff_size - len, "</MBusData>\n");

return buff;

}
//------------------------------------------------------------------------------
/// Return a string containing an XML representation of the M-BUS frame data.
//------------------------------------------------------------------------------
char *
mbus_frame_data_xml(mbus_frame_data *data)
{
if (data)
{
if (data->type == MBUS_DATA_TYPE_ERROR)
{
return mbus_data_error_xml(data->error);
}
    if (data->type == MBUS_DATA_TYPE_FIXED)
    {
        return mbus_data_fixed_xml(&(data->data_fix));
    }

    if (data->type == MBUS_DATA_TYPE_VARIABLE)
    {
        return mbus_data_variable_xml(&(data->data_var));
    }
}

return NULL;

}
//------------------------------------------------------------------------------
/// Return an XML representation of the M-BUS frame.
//------------------------------------------------------------------------------
char *
mbus_frame_xml(mbus_frame *frame)
{
mbus_frame_data frame_data;
mbus_frame *iter;
mbus_data_record *record;
char *buff = NULL, *new_buff;

size_t len = 0, buff_size = 8192;
int record_cnt = 0, frame_cnt;

if (frame)
{
    memset((void *)&frame_data, 0, sizeof(mbus_frame_data));

    if (mbus_frame_data_parse(frame, &frame_data) == -1)
    {
        mbus_error_str_set("M-bus data parse error.");
        return NULL;
    }

    if (frame_data.type == MBUS_DATA_TYPE_ERROR)
    {
        //
        // generate XML for error
        //
        return mbus_data_error_xml(frame_data.error);
    }

    if (frame_data.type == MBUS_DATA_TYPE_FIXED)
    {
        //
        // generate XML for fixed data
        //
        return mbus_data_fixed_xml(&(frame_data.data_fix));
    }

    if (frame_data.type == MBUS_DATA_TYPE_VARIABLE)
    {
        //
        // generate XML for a sequence of variable data frames
        //

        buff = (char*) malloc(buff_size);

        if (buff == NULL)
        {
            mbus_data_record_free(frame_data.data_var.record);
            return NULL;
        }

        // include frame counter in XML output if more than one frame
        // is available (frame_cnt = -1 => not included in output)
        frame_cnt = (frame->next == NULL) ? -1 : 0;

        len += snprintf(&buff[len], buff_size - len, MBUS_XML_PROCESSING_INSTRUCTION);

        len += snprintf(&buff[len], buff_size - len, "<MBusData>\n\n");

        // only print the header info for the first frame (should be
        // the same for each frame in a sequence of a multi-telegram
        // transfer.
        len += snprintf(&buff[len], buff_size - len, "%s",
                                mbus_data_variable_header_xml(&(frame_data.data_var.header)));

        // loop through all records in the current frame, using a global
        // record count as record ID in the XML output
        for (record = frame_data.data_var.record; record; record = record->next, record_cnt++)
        {
            if ((buff_size - len) < 1024)
            {
                buff_size *= 2;
                new_buff = (char*) realloc(buff,buff_size);

                if (new_buff == NULL)
                {
                    free(buff);
                    mbus_data_record_free(frame_data.data_var.record);
                    return NULL;
                }

                buff = new_buff;
            }

            len += snprintf(&buff[len], buff_size - len, "%s",
                            mbus_data_variable_record_xml(record, record_cnt, frame_cnt, &(frame_data.data_var.header)));
        }

        // free all records in the list
        if (frame_data.data_var.record)
        {
            mbus_data_record_free(frame_data.data_var.record);
        }

        frame_cnt++;

        for (iter = frame->next; iter; iter = iter->next, frame_cnt++)
        {
            if (mbus_frame_data_parse(iter, &frame_data) == -1)
            {
                mbus_error_str_set("M-bus variable data parse error.");
                return NULL;
            }

            // loop through all records in the current frame, using a global
            // record count as record ID in the XML output
            for (record = frame_data.data_var.record; record; record = record->next, record_cnt++)
            {
                if ((buff_size - len) < 1024)
                {
                    buff_size *= 2;
                    new_buff = (char*) realloc(buff,buff_size);

                    if (new_buff == NULL)
                    {
                        free(buff);
                        mbus_data_record_free(frame_data.data_var.record);
                        return NULL;
                    }

                    buff = new_buff;
                }

                len += snprintf(&buff[len], buff_size - len, "%s",
                                mbus_data_variable_record_xml(record, record_cnt, frame_cnt, &(frame_data.data_var.header)));
            }

            // free all records in the list
            if (frame_data.data_var.record)
            {
                mbus_data_record_free(frame_data.data_var.record);
            }
        }

        len += snprintf(&buff[len], buff_size - len, "</MBusData>\n");

        return buff;
    }
}

return NULL;

}
//------------------------------------------------------------------------------
/// Allocate and initialize a new frame data structure
//------------------------------------------------------------------------------
mbus_frame_data *
mbus_frame_data_new()
{
mbus_frame_data *data;
if ((data = (mbus_frame_data *)malloc(sizeof(mbus_frame_data))) == NULL)
{
    return NULL;
}

memset(data, 0, sizeof(mbus_frame_data));

data->data_var.data = NULL;
data->data_var.record = NULL;

return data;

}
//-----------------------------------------------------------------------------
/// Free up data associated with a frame data structure
//------------------------------------------------------------------------------
void
mbus_frame_data_free(mbus_frame_data *data)
{
if (data)
{
if (data->data_var.record)
{
mbus_data_record_free(data->data_var.record); // free's up the whole list
}
    free(data);
}

}
//------------------------------------------------------------------------------
/// Allocate and initialize a new variable data record
//------------------------------------------------------------------------------
mbus_data_record *
mbus_data_record_new()
{
mbus_data_record *record;
if ((record = (mbus_data_record *)malloc(sizeof(mbus_data_record))) == NULL)
{
    return NULL;
}

memset(record, 0, sizeof(mbus_data_record));

record->next = NULL;
return record;

}
//------------------------------------------------------------------------------
/// free up memory associated with a data record and all the subsequent records
/// in its list (apply recursively)
//------------------------------------------------------------------------------
void
mbus_data_record_free(mbus_data_record *record)
{
if (record)
{
mbus_data_record *next = record->next;
    free(record);

    if (next)
        mbus_data_record_free(next);
}

}
//------------------------------------------------------------------------------
/// Return a string containing an XML representation of the M-BUS frame.
//------------------------------------------------------------------------------
void
mbus_data_record_append(mbus_data_variable *data, mbus_data_record *record)
{
mbus_data_record *iter;
if (data && record)
{
    if (data->record == NULL)
    {
        data->record = record;
    }
    else
    {
        // find the end of the list
        for (iter = data->record; iter->next; iter = iter->next);

        iter->next = record;
    }
}

}
//------------------------------------------------------------------------------
// Extract the secondary address from an M-Bus frame. The secondary address
// should be a 16 character string comprised of the device ID (4 bytes),
// manufacturer ID (2 bytes), version (1 byte) and medium (1 byte).
//------------------------------------------------------------------------------
char *
mbus_frame_get_secondary_address(mbus_frame *frame)
{
static char addr
32
;
mbus_frame_data *data;
unsigned long id;
if (frame == NULL || (data = mbus_frame_data_new()) == NULL)
{
    snprintf(error_str, sizeof(error_str),
             "%s: Failed to allocate data structure [%p, %p].",
              __PRETTY_FUNCTION__, (void*)frame, (void*)data);
    return NULL;
}

if (frame->control_information != MBUS_CONTROL_INFO_RESP_VARIABLE)
{
    snprintf(error_str, sizeof(error_str), "Non-variable data response (can't get secondary address from response).");
    mbus_frame_data_free(data);
    return NULL;
}

if (mbus_frame_data_parse(frame, data) == -1)
{
    mbus_frame_data_free(data);
    return NULL;
}

id = (unsigned long) mbus_data_bcd_decode_hex(data->data_var.header.id_bcd, 4);

snprintf(addr, sizeof(addr), "%08lX%02X%02X%02X%02X",
         id,
         data->data_var.header.manufacturer[0],
         data->data_var.header.manufacturer[1],
         data->data_var.header.version,
         data->data_var.header.medium);

// free data
mbus_frame_data_free(data);

return addr;

}
//------------------------------------------------------------------------------
// Pack a secondary address string into an mbus frame
//------------------------------------------------------------------------------
int
mbus_frame_select_secondary_pack(mbus_frame *frame, char *address)
{
int val, i, j, k;
char tmp
16
;
if (frame == NULL || address == NULL)
{
    snprintf(error_str, sizeof(error_str), "%s: frame or address arguments are NULL.", __PRETTY_FUNCTION__);
    return -1;
}

if (mbus_is_secondary_address(address) == 0)
{
    snprintf(error_str, sizeof(error_str), "%s: address is invalid.", __PRETTY_FUNCTION__);
    return -1;
}

frame->control  = MBUS_CONTROL_MASK_SND_UD | MBUS_CONTROL_MASK_DIR_M2S | MBUS_CONTROL_MASK_FCB;
frame->address  = MBUS_ADDRESS_NETWORK_LAYER;             // for addressing secondary slaves
frame->control_information = MBUS_CONTROL_INFO_SELECT_SLAVE; // mode 1

frame->data_size = 8;

// parse secondary_addr_str and populate frame->data[0-7]
// ex: secondary_addr_str = "14491001 1057 01 06"
// (excluding the blank spaces)

strncpy(tmp, &address[14], 2); tmp[2] = 0;
val = strtol(tmp, NULL, 16);
frame->data[7] = val & 0xFF;

strncpy(tmp, &address[12], 2); tmp[2] = 0;
val = strtol(tmp, NULL, 16);
frame->data[6] = val & 0xFF;

strncpy(tmp,  &address[8], 4); tmp[4] = 0;
val = strtol(tmp, NULL, 16);
frame->data[4] = (val>>8) & 0xFF;
frame->data[5] =  val     & 0xFF;

// parse the ID string, allowing for F wildcard characters.
frame->data[0] = 0;
frame->data[1] = 0;
frame->data[2] = 0;
frame->data[3] = 0;
j = 3; k = 1;
for (i = 0; i < 8; i++)
{
    if (address[i] == 'F' || address[i] == 'f')
    {
        frame->data[j] |= 0x0F << (4 * k--);
    }
    else
    {
        frame->data[j] |= (0x0F & (address[i] - '0')) << (4 * k--);
    }

    if (k < 0)
    {
        k = 1; j--;
    }
}

return 0;

}
//---------------------------------------------------------
// Checks if an integer is a valid primary address.
//---------------------------------------------------------
int
mbus_is_primary_address(int value)
{
return ((value >= 0x00) && (value <= 0xFF));
}
//---------------------------------------------------------
// Checks if an string is a valid secondary address.
//---------------------------------------------------------
int
mbus_is_secondary_address(const char * value)
{
int i;
if (value == NULL)
    return 0;

if (strlen(value) != 16)
    return 0;

for (i = 0; i < 16; i++)
{
    if (!isxdigit(value[i]))
        return 0;
}

return 1;

}



The other file is called mbus-protocol-aux.c:

//------------------------------------------------------------------------------
// Copyright (C) 2010-2011, Robert Johansson and contributors, Raditex AB
// All rights reserved.
//
// rSCADA
// http://www.rSCADA.se
// info@rscada.se
//
// Contributors:
// Large parts of this file was contributed by Tomas Menzl.
//
//------------------------------------------------------------------------------
#include "mbus-protocol-aux.h"
#include "mbus-serial.h"
#include "mbus-tcp.h"
#include <stdio.h>
#include <string.h>
#include <ctype.h>
#include <math.h>
/@ignore@/
#define MBUS_ERROR(...) fprintf (stderr, VA_ARGS)
#ifdef DEBUG
#define MBUS_DEBUG(...) fprintf (stderr, VA_ARGS)
#else
#define MBUS_DEBUG(...)
#endif
/@end@/
static int debug = 0;
typedef struct _mbus_variable_vif {
unsigned vif;
double exponent;
const char * unit;
const char * quantity;
} mbus_variable_vif;
mbus_variable_vif vif_table[] = {
/* Primary VIFs (main table), range 0x00 - 0xFF */
/*  E000 0nnn    Energy Wh (0.001Wh to 10000Wh) */
{ 0x00, 1.0e-3, "Wh", "Energy" },
{ 0x01, 1.0e-2, "Wh", "Energy" },
{ 0x02, 1.0e-1, "Wh", "Energy" },
{ 0x03, 1.0e0,  "Wh", "Energy" },
{ 0x04, 1.0e1,  "Wh", "Energy" },
{ 0x05, 1.0e2,  "Wh", "Energy" },
{ 0x06, 1.0e3,  "Wh", "Energy" },
{ 0x07, 1.0e4,  "Wh", "Energy" },

/* E000 1nnn    Energy  J (0.001kJ to 10000kJ) */
{ 0x08, 1.0e0, "J", "Energy" },
{ 0x09, 1.0e1, "J", "Energy" },
{ 0x0A, 1.0e2, "J", "Energy" },
{ 0x0B, 1.0e3, "J", "Energy" },
{ 0x0C, 1.0e4, "J", "Energy" },
{ 0x0D, 1.0e5, "J", "Energy" },
{ 0x0E, 1.0e6, "J", "Energy" },
{ 0x0F, 1.0e7, "J", "Energy" },

/* E001 0nnn    Volume m^3 (0.001l to 10000l) */
{ 0x10, 1.0e-6, "m^3", "Volume" },
{ 0x11, 1.0e-5, "m^3", "Volume" },
{ 0x12, 1.0e-4, "m^3", "Volume" },
{ 0x13, 1.0e-3, "m^3", "Volume" },
{ 0x14, 1.0e-2, "m^3", "Volume" },
{ 0x15, 1.0e-1, "m^3", "Volume" },
{ 0x16, 1.0e0,  "m^3", "Volume" },
{ 0x17, 1.0e1,  "m^3", "Volume" },

/* E001 1nnn    Mass kg (0.001kg to 10000kg) */
{ 0x18, 1.0e-3, "kg", "Mass" },
{ 0x19, 1.0e-2, "kg", "Mass" },
{ 0x1A, 1.0e-1, "kg", "Mass" },
{ 0x1B, 1.0e0,  "kg", "Mass" },
{ 0x1C, 1.0e1,  "kg", "Mass" },
{ 0x1D, 1.0e2,  "kg", "Mass" },
{ 0x1E, 1.0e3,  "kg", "Mass" },
{ 0x1F, 1.0e4,  "kg", "Mass" },

/* E010 00nn    On Time s */
{ 0x20,     1.0, "s", "On time" },  /* seconds */
{ 0x21,    60.0, "s", "On time" },  /* minutes */
{ 0x22,  3600.0, "s", "On time" },  /* hours   */
{ 0x23, 86400.0, "s", "On time" },  /* days    */

/* E010 01nn    Operating Time s */
{ 0x24,     1.0, "s", "Operating time" },  /* seconds */
{ 0x25,    60.0, "s", "Operating time" },  /* minutes */
{ 0x26,  3600.0, "s", "Operating time" },  /* hours   */
{ 0x27, 86400.0, "s", "Operating time" },  /* days    */

/* E010 1nnn    Power W (0.001W to 10000W) */
{ 0x28, 1.0e-3, "W", "Power" },
{ 0x29, 1.0e-2, "W", "Power" },
{ 0x2A, 1.0e-1, "W", "Power" },
{ 0x2B, 1.0e0,  "W", "Power" },
{ 0x2C, 1.0e1,  "W", "Power" },
{ 0x2D, 1.0e2,  "W", "Power" },
{ 0x2E, 1.0e3,  "W", "Power" },
{ 0x2F, 1.0e4,  "W", "Power" },

/* E011 0nnn    Power J/h (0.001kJ/h to 10000kJ/h) */
{ 0x30, 1.0e0, "J/h", "Power" },
{ 0x31, 1.0e1, "J/h", "Power" },
{ 0x32, 1.0e2, "J/h", "Power" },
{ 0x33, 1.0e3, "J/h", "Power" },
{ 0x34, 1.0e4, "J/h", "Power" },
{ 0x35, 1.0e5, "J/h", "Power" },
{ 0x36, 1.0e6, "J/h", "Power" },
{ 0x37, 1.0e7, "J/h", "Power" },

/* E011 1nnn    Volume Flow m3/h (0.001l/h to 10000l/h) */
{ 0x38, 1.0e-6, "m^3/h", "Volume flow" },
{ 0x39, 1.0e-5, "m^3/h", "Volume flow" },
{ 0x3A, 1.0e-4, "m^3/h", "Volume flow" },
{ 0x3B, 1.0e-3, "m^3/h", "Volume flow" },
{ 0x3C, 1.0e-2, "m^3/h", "Volume flow" },
{ 0x3D, 1.0e-1, "m^3/h", "Volume flow" },
{ 0x3E, 1.0e0,  "m^3/h", "Volume flow" },
{ 0x3F, 1.0e1,  "m^3/h", "Volume flow" },

/* E100 0nnn     Volume Flow ext.  m^3/min (0.0001l/min to 1000l/min) */
{ 0x40, 1.0e-7, "m^3/min", "Volume flow" },
{ 0x41, 1.0e-6, "m^3/min", "Volume flow" },
{ 0x42, 1.0e-5, "m^3/min", "Volume flow" },
{ 0x43, 1.0e-4, "m^3/min", "Volume flow" },
{ 0x44, 1.0e-3, "m^3/min", "Volume flow" },
{ 0x45, 1.0e-2, "m^3/min", "Volume flow" },
{ 0x46, 1.0e-1, "m^3/min", "Volume flow" },
{ 0x47, 1.0e0,  "m^3/min", "Volume flow" },

/* E100 1nnn     Volume Flow ext.  m^3/s (0.001ml/s to 10000ml/s) */
{ 0x48, 1.0e-9, "m^3/s", "Volume flow" },
{ 0x49, 1.0e-8, "m^3/s", "Volume flow" },
{ 0x4A, 1.0e-7, "m^3/s", "Volume flow" },
{ 0x4B, 1.0e-6, "m^3/s", "Volume flow" },
{ 0x4C, 1.0e-5, "m^3/s", "Volume flow" },
{ 0x4D, 1.0e-4, "m^3/s", "Volume flow" },
{ 0x4E, 1.0e-3, "m^3/s", "Volume flow" },
{ 0x4F, 1.0e-2, "m^3/s", "Volume flow" },

/* E101 0nnn     Mass flow kg/h (0.001kg/h to 10000kg/h) */
{ 0x50, 1.0e-3, "kg/h", "Mass flow" },
{ 0x51, 1.0e-2, "kg/h", "Mass flow" },
{ 0x52, 1.0e-1, "kg/h", "Mass flow" },
{ 0x53, 1.0e0,  "kg/h", "Mass flow" },
{ 0x54, 1.0e1,  "kg/h", "Mass flow" },
{ 0x55, 1.0e2,  "kg/h", "Mass flow" },
{ 0x56, 1.0e3,  "kg/h", "Mass flow" },
{ 0x57, 1.0e4,  "kg/h", "Mass flow" },

/* E101 10nn     Flow Temperature °C (0.001°C to 1°C) */
{ 0x58, 1.0e-3, "°C", "Flow temperature" },
{ 0x59, 1.0e-2, "°C", "Flow temperature" },
{ 0x5A, 1.0e-1, "°C", "Flow temperature" },
{ 0x5B, 1.0e0,  "°C", "Flow temperature" },

/* E101 11nn Return Temperature °C (0.001°C to 1°C) */
{ 0x5C, 1.0e-3, "°C", "Return temperature" },
{ 0x5D, 1.0e-2, "°C", "Return temperature" },
{ 0x5E, 1.0e-1, "°C", "Return temperature" },
{ 0x5F, 1.0e0,  "°C", "Return temperature" },

/* E110 00nn    Temperature Difference  K   (mK to  K) */
{ 0x60, 1.0e-3, "K", "Temperature difference" },
{ 0x61, 1.0e-2, "K", "Temperature difference" },
{ 0x62, 1.0e-1, "K", "Temperature difference" },
{ 0x63, 1.0e0,  "K", "Temperature difference" },

/* E110 01nn     External Temperature °C (0.001°C to 1°C) */
{ 0x64, 1.0e-3, "°C", "External temperature" },
{ 0x65, 1.0e-2, "°C", "External temperature" },
{ 0x66, 1.0e-1, "°C", "External temperature" },
{ 0x67, 1.0e0,  "°C", "External temperature" },

/* E110 10nn     Pressure bar (1mbar to 1000mbar) */
{ 0x68, 1.0e-3, "bar", "Pressure" },
{ 0x69, 1.0e-2, "bar", "Pressure" },
{ 0x6A, 1.0e-1, "bar", "Pressure" },
{ 0x6B, 1.0e0,  "bar", "Pressure" },

/* E110 110n     Time Point */
{ 0x6C, 1.0e0, "-", "Time point (date)" },            /* n = 0        date, data type G */
{ 0x6D, 1.0e0, "-", "Time point (date & time)" },     /* n = 1 time & date, data type F */

/* E110 1110     Units for H.C.A. dimensionless */
{ 0x6E, 1.0e0,  "Units for H.C.A.", "H.C.A." },

/* E110 1111     Reserved */
{ 0x6F, 0.0,  "Reserved", "Reserved" },

/* E111 00nn     Averaging Duration s */
{ 0x70,     1.0, "s", "Averaging Duration" },  /* seconds */
{ 0x71,    60.0, "s", "Averaging Duration" },  /* minutes */
{ 0x72,  3600.0, "s", "Averaging Duration" },  /* hours   */
{ 0x73, 86400.0, "s", "Averaging Duration" },  /* days    */

/* E111 01nn     Actuality Duration s */
{ 0x74,     1.0, "s", "Actuality Duration" },  /* seconds */
{ 0x75,    60.0, "s", "Actuality Duration" },  /* minutes */
{ 0x76,  3600.0, "s", "Actuality Duration" },  /* hours   */
{ 0x77, 86400.0, "s", "Actuality Duration" },  /* days    */

/* Fabrication No */
{ 0x78, 1.0, "", "Fabrication No" },

/* E111 1001 (Enhanced) Identification */
{ 0x79, 1.0, "", "(Enhanced) Identification" },

/* E111 1010 Bus Address */
{ 0x7A, 1.0, "", "Bus Address" },

/* Any VIF: 7Eh */
{ 0x7E, 1.0, "", "Any VIF" },

/* Manufacturer specific: 7Fh */
{ 0x7F, 1.0, "", "Manufacturer specific" },

/* Any VIF: 7Eh */
{ 0xFE, 1.0, "", "Any VIF" },

/* Manufacturer specific: FFh */
{ 0xFF, 1.0, "", "Manufacturer specific" },

/* Main VIFE-Code Extension table (following VIF=FDh for primary VIF)
See 8.4.4 a, only some of them are here. Using range 0x100 - 0x1FF */
/* E000 00nn   Credit of 10nn-3 of the nominal local legal currency units */
{ 0x100, 1.0e-3, "Currency units", "Credit" },
{ 0x101, 1.0e-2, "Currency units", "Credit" },
{ 0x102, 1.0e-1, "Currency units", "Credit" },
{ 0x103, 1.0e0,  "Currency units", "Credit" },

/* E000 01nn   Debit of 10nn-3 of the nominal local legal currency units */
{ 0x104, 1.0e-3, "Currency units", "Debit" },
{ 0x105, 1.0e-2, "Currency units", "Debit" },
{ 0x106, 1.0e-1, "Currency units", "Debit" },
{ 0x107, 1.0e0,  "Currency units", "Debit" },

/* E000 1000 Access Number (transmission count) */
{ 0x108, 1.0e0,  "", "Access Number (transmission count)" },

/* E000 1001 Medium (as in fixed header) */
{ 0x109, 1.0e0,  "", "Medium" },

/* E000 1010 Manufacturer (as in fixed header) */
{ 0x10A, 1.0e0,  "", "Manufacturer" },

/* E000 1011 Parameter set identification */
{ 0x10B, 1.0e0,  "", "Parameter set identification" },

/* E000 1100 Model / Version */
{ 0x10C, 1.0e0,  "", "Model / Version" },

/* E000 1101 Hardware version # */
{ 0x10D, 1.0e0,  "", "Hardware version" },

/* E000 1110 Firmware version # */
{ 0x10E, 1.0e0,  "", "Firmware version" },

/* E000 1111 Software version # */
{ 0x10F, 1.0e0,  "", "Software version" },


/* E001 0000 Customer location */
{ 0x110, 1.0e0,  "", "Customer location" },

/* E001 0001 Customer */
{ 0x111, 1.0e0,  "", "Customer" },

/* E001 0010 Access Code User */
{ 0x112, 1.0e0,  "", "Access Code User" },

/* E001 0011 Access Code Operator */
{ 0x113, 1.0e0,  "", "Access Code Operator" },

/* E001 0100 Access Code System Operator */
{ 0x114, 1.0e0,  "", "Access Code System Operator" },

/* E001 0101 Access Code Developer */
{ 0x115, 1.0e0,  "", "Access Code Developer" },

/* E001 0110 Password */
{ 0x116, 1.0e0,  "", "Password" },

/* E001 0111 Error flags (binary) */
{ 0x117, 1.0e0,  "", "Error flags" },

/* E001 1000 Error mask */
{ 0x118, 1.0e0,  "", "Error mask" },

/* E001 1001 Reserved */
{ 0x119, 1.0e0,  "Reserved", "Reserved" },


/* E001 1010 Digital Output (binary) */
{ 0x11A, 1.0e0,  "", "Digital Output" },

/* E001 1011 Digital Input (binary) */
{ 0x11B, 1.0e0,  "", "Digital Input" },

/* E001 1100 Baudrate [Baud] */
{ 0x11C, 1.0e0,  "Baud", "Baudrate" },

/* E001 1101 Response delay time [bittimes] */
{ 0x11D, 1.0e0,  "Bittimes", "Response delay time" },

/* E001 1110 Retry */
{ 0x11E, 1.0e0,  "", "Retry" },

/* E001 1111 Reserved */
{ 0x11F, 1.0e0,  "Reserved", "Reserved" },


/* E010 0000 First storage # for cyclic storage */
{ 0x120, 1.0e0,  "", "First storage # for cyclic storage" },

/* E010 0001 Last storage # for cyclic storage */
{ 0x121, 1.0e0,  "", "Last storage # for cyclic storage" },

/* E010 0010 Size of storage block */
{ 0x122, 1.0e0,  "", "Size of storage block" },

/* E010 0011 Reserved */
{ 0x123, 1.0e0,  "Reserved", "Reserved" },

/* E010 01nn Storage interval [sec(s)..day(s)] */
{ 0x124,        1.0,  "s", "Storage interval" },   /* second(s) */
{ 0x125,       60.0,  "s", "Storage interval" },   /* minute(s) */
{ 0x126,     3600.0,  "s", "Storage interval" },   /* hour(s)   */
{ 0x127,    86400.0,  "s", "Storage interval" },   /* day(s)    */
{ 0x128,  2629743.83, "s", "Storage interval" },   /* month(s)  */
{ 0x129, 31556926.0,  "s", "Storage interval" },   /* year(s)   */

/* E010 1010 Reserved */
{ 0x12A, 1.0e0,  "Reserved", "Reserved" },

/* E010 1011 Reserved */
{ 0x12B, 1.0e0,  "Reserved", "Reserved" },

/* E010 11nn Duration since last readout [sec(s)..day(s)] */
{ 0x12C,     1.0, "s", "Duration since last readout" },  /* seconds */
{ 0x12D,    60.0, "s", "Duration since last readout" },  /* minutes */
{ 0x12E,  3600.0, "s", "Duration since last readout" },  /* hours   */
{ 0x12F, 86400.0, "s", "Duration since last readout" },  /* days    */

/* E011 0000 Start (date/time) of tariff  */
/* The information about usage of data type F (date and time) or data type G (date) can */
/* be derived from the datafield (0010b: type G / 0100: type F). */
{ 0x130, 1.0e0,  "Reserved", "Reserved" }, /* ???? */

/* E011 00nn Duration of tariff (nn=01 ..11: min to days) */
{ 0x131,       60.0,  "s", "Duration of tariff" },   /* minute(s) */
{ 0x132,     3600.0,  "s", "Duration of tariff" },   /* hour(s)   */
{ 0x133,    86400.0,  "s", "Duration of tariff" },   /* day(s)    */

/* E011 01nn Period of tariff [sec(s) to day(s)]  */
{ 0x134,        1.0, "s", "Period of tariff" },  /* seconds  */
{ 0x135,       60.0, "s", "Period of tariff" },  /* minutes  */
{ 0x136,     3600.0, "s", "Period of tariff" },  /* hours    */
{ 0x137,    86400.0, "s", "Period of tariff" },  /* days     */
{ 0x138,  2629743.83,"s", "Period of tariff" },  /* month(s) */
{ 0x139, 31556926.0, "s", "Period of tariff" },  /* year(s)  */

/* E011 1010 dimensionless / no VIF */
{ 0x13A, 1.0e0,  "", "Dimensionless" },

/* E011 1011 Reserved */
{ 0x13B, 1.0e0,  "Reserved", "Reserved" },

/* E011 11xx Reserved */
{ 0x13C, 1.0e0,  "Reserved", "Reserved" },
{ 0x13D, 1.0e0,  "Reserved", "Reserved" },
{ 0x13E, 1.0e0,  "Reserved", "Reserved" },
{ 0x13F, 1.0e0,  "Reserved", "Reserved" },

/* E100 nnnn   Volts electrical units */
{ 0x140, 1.0e-9, "V", "Voltage" },
{ 0x141, 1.0e-8, "V", "Voltage" },
{ 0x142, 1.0e-7, "V", "Voltage" },
{ 0x143, 1.0e-6, "V", "Voltage" },
{ 0x144, 1.0e-5, "V", "Voltage" },
{ 0x145, 1.0e-4, "V", "Voltage" },
{ 0x146, 1.0e-3, "V", "Voltage" },
{ 0x147, 1.0e-2, "V", "Voltage" },
{ 0x148, 1.0e-1, "V", "Voltage" },
{ 0x149, 1.0e0,  "V", "Voltage" },
{ 0x14A, 1.0e1,  "V", "Voltage" },
{ 0x14B, 1.0e2,  "V", "Voltage" },
{ 0x14C, 1.0e3,  "V", "Voltage" },
{ 0x14D, 1.0e4,  "V", "Voltage" },
{ 0x14E, 1.0e5,  "V", "Voltage" },
{ 0x14F, 1.0e6,  "V", "Voltage" },

/* E101 nnnn   A */
{ 0x150, 1.0e-12, "A", "Current" },
{ 0x151, 1.0e-11, "A", "Current" },
{ 0x152, 1.0e-10, "A", "Current" },
{ 0x153, 1.0e-9,  "A", "Current" },
{ 0x154, 1.0e-8,  "A", "Current" },
{ 0x155, 1.0e-7,  "A", "Current" },
{ 0x156, 1.0e-6,  "A", "Current" },
{ 0x157, 1.0e-5,  "A", "Current" },
{ 0x158, 1.0e-4,  "A", "Current" },
{ 0x159, 1.0e-3,  "A", "Current" },
{ 0x15A, 1.0e-2,  "A", "Current" },
{ 0x15B, 1.0e-1,  "A", "Current" },
{ 0x15C, 1.0e0,   "A", "Current" },
{ 0x15D, 1.0e1,   "A", "Current" },
{ 0x15E, 1.0e2,   "A", "Current" },
{ 0x15F, 1.0e3,   "A", "Current" },

/* E110 0000 Reset counter */
{ 0x160, 1.0e0,  "", "Reset counter" },

/* E110 0001 Cumulation counter */
{ 0x161, 1.0e0,  "", "Cumulation counter" },

/* E110 0010 Control signal */
{ 0x162, 1.0e0,  "", "Control signal" },

/* E110 0011 Day of week */
{ 0x163, 1.0e0,  "", "Day of week" },

/* E110 0100 Week number */
{ 0x164, 1.0e0,  "", "Week number" },

/* E110 0101 Time point of day change */
{ 0x165, 1.0e0,  "", "Time point of day change" },

/* E110 0110 State of parameter activation */
{ 0x166, 1.0e0,  "", "State of parameter activation" },

/* E110 0111 Special supplier information */
{ 0x167, 1.0e0,  "", "Special supplier information" },

/* E110 10pp Duration since last cumulation [hour(s)..years(s)] */
{ 0x168,     3600.0, "s", "Duration since last cumulation" },  /* hours    */
{ 0x169,    86400.0, "s", "Duration since last cumulation" },  /* days     */
{ 0x16A,  2629743.83,"s", "Duration since last cumulation" },  /* month(s) */
{ 0x16B, 31556926.0, "s", "Duration since last cumulation" },  /* year(s)  */

/* E110 11pp Operating time battery [hour(s)..years(s)] */
{ 0x16C,     3600.0, "s", "Operating time battery" },  /* hours    */
{ 0x16D,    86400.0, "s", "Operating time battery" },  /* days     */
{ 0x16E,  2629743.83,"s", "Operating time battery" },  /* month(s) */
{ 0x16F, 31556926.0, "s", "Operating time battery" },  /* year(s)  */

/* E111 0000 Date and time of battery change */
{ 0x170, 1.0e0,  "", "Date and time of battery change" },

/* E111 0001-1111 Reserved */
{ 0x171, 1.0e0,  "Reserved", "Reserved" },
{ 0x172, 1.0e0,  "Reserved", "Reserved" },
{ 0x173, 1.0e0,  "Reserved", "Reserved" },
{ 0x174, 1.0e0,  "Reserved", "Reserved" },
{ 0x175, 1.0e0,  "Reserved", "Reserved" },
{ 0x176, 1.0e0,  "Reserved", "Reserved" },
{ 0x177, 1.0e0,  "Reserved", "Reserved" },
{ 0x178, 1.0e0,  "Reserved", "Reserved" },
{ 0x179, 1.0e0,  "Reserved", "Reserved" },
{ 0x17A, 1.0e0,  "Reserved", "Reserved" },
{ 0x17B, 1.0e0,  "Reserved", "Reserved" },
{ 0x17C, 1.0e0,  "Reserved", "Reserved" },
{ 0x17D, 1.0e0,  "Reserved", "Reserved" },
{ 0x17E, 1.0e0,  "Reserved", "Reserved" },
{ 0x17F, 1.0e0,  "Reserved", "Reserved" },

/* Alternate VIFE-Code Extension table (following VIF=0FBh for primary VIF)
See 8.4.4 b, only some of them are here. Using range 0x200 - 0x2FF */
/* E000 000n Energy 10(n-1) MWh 0.1MWh to 1MWh */
{ 0x200, 1.0e5,  "Wh", "Energy" },
{ 0x201, 1.0e6,  "Wh", "Energy" },

/* E000 001n Reserved */
{ 0x202, 1.0e0,  "Reserved", "Reserved" },
{ 0x203, 1.0e0,  "Reserved", "Reserved" },

/* E000 01nn Reserved */
{ 0x204, 1.0e0,  "Reserved", "Reserved" },
{ 0x205, 1.0e0,  "Reserved", "Reserved" },
{ 0x206, 1.0e0,  "Reserved", "Reserved" },
{ 0x207, 1.0e0,  "Reserved", "Reserved" },

/* E000 100n Energy 10(n-1) GJ 0.1GJ to 1GJ */
{ 0x208, 1.0e8,  "Reserved", "Energy" },
{ 0x209, 1.0e9,  "Reserved", "Energy" },

/* E000 101n Reserved */
{ 0x20A, 1.0e0,  "Reserved", "Reserved" },
{ 0x20B, 1.0e0,  "Reserved", "Reserved" },

/* E000 11nn Reserved */
{ 0x20C, 1.0e0,  "Reserved", "Reserved" },
{ 0x20D, 1.0e0,  "Reserved", "Reserved" },
{ 0x20E, 1.0e0,  "Reserved", "Reserved" },
{ 0x20F, 1.0e0,  "Reserved", "Reserved" },

/* E001 000n Volume 10(n+2) m3 100m3 to 1000m3 */
{ 0x210, 1.0e2,  "m^3", "Volume" },
{ 0x211, 1.0e3,  "m^3", "Volume" },

/* E001 001n Reserved */
{ 0x212, 1.0e0,  "Reserved", "Reserved" },
{ 0x213, 1.0e0,  "Reserved", "Reserved" },

/* E001 01nn Reserved */
{ 0x214, 1.0e0,  "Reserved", "Reserved" },
{ 0x215, 1.0e0,  "Reserved", "Reserved" },
{ 0x216, 1.0e0,  "Reserved", "Reserved" },
{ 0x217, 1.0e0,  "Reserved", "Reserved" },

/* E001 100n Mass 10(n+2) t 100t to 1000t */
{ 0x218, 1.0e5,  "kg", "Mass" },
{ 0x219, 1.0e6,  "kg", "Mass" },

/* E001 1010 to E010 0000 Reserved */
{ 0x21A, 1.0e0,  "Reserved", "Reserved" },
{ 0x21B, 1.0e0,  "Reserved", "Reserved" },
{ 0x21C, 1.0e0,  "Reserved", "Reserved" },
{ 0x21D, 1.0e0,  "Reserved", "Reserved" },
{ 0x21E, 1.0e0,  "Reserved", "Reserved" },
{ 0x21F, 1.0e0,  "Reserved", "Reserved" },
{ 0x220, 1.0e0,  "Reserved", "Reserved" },

/* E010 0001 Volume 0,1 feet^3 */
{ 0x221, 1.0e-1, "feet^3", "Volume" },

/* E010 001n Volume 0,1-1 american gallon */
{ 0x222, 1.0e-1, "American gallon", "Volume" },
{ 0x223, 1.0e-0, "American gallon", "Volume" },

/* E010 0100    Volume flow 0,001 american gallon/min */
{ 0x224, 1.0e-3, "American gallon/min", "Volume flow" },

/* E010 0101 Volume flow 1 american gallon/min */
{ 0x225, 1.0e0,  "American gallon/min", "Volume flow" },

/* E010 0110 Volume flow 1 american gallon/h */
{ 0x226, 1.0e0,  "American gallon/h", "Volume flow" },

/* E010 0111 Reserved */
{ 0x227, 1.0e0, "Reserved", "Reserved" },

/* E010 100n Power 10(n-1) MW 0.1MW to 1MW */
{ 0x228, 1.0e5, "W", "Power" },
{ 0x229, 1.0e6, "W", "Power" },

/* E010 101n Reserved */
{ 0x22A, 1.0e0, "Reserved", "Reserved" },
{ 0x22B, 1.0e0, "Reserved", "Reserved" },

/* E010 11nn Reserved */
{ 0x22C, 1.0e0, "Reserved", "Reserved" },
{ 0x22D, 1.0e0, "Reserved", "Reserved" },
{ 0x22E, 1.0e0, "Reserved", "Reserved" },
{ 0x22F, 1.0e0, "Reserved", "Reserved" },

/* E011 000n Power 10(n-1) GJ/h 0.1GJ/h to 1GJ/h */
{ 0x230, 1.0e8, "J", "Power" },
{ 0x231, 1.0e9, "J", "Power" },

/* E011 0010 to E101 0111 Reserved */
{ 0x232, 1.0e0, "Reserved", "Reserved" },
{ 0x233, 1.0e0, "Reserved", "Reserved" },
{ 0x234, 1.0e0, "Reserved", "Reserved" },
{ 0x235, 1.0e0, "Reserved", "Reserved" },
{ 0x236, 1.0e0, "Reserved", "Reserved" },
{ 0x237, 1.0e0, "Reserved", "Reserved" },
{ 0x238, 1.0e0, "Reserved", "Reserved" },
{ 0x239, 1.0e0, "Reserved", "Reserved" },
{ 0x23A, 1.0e0, "Reserved", "Reserved" },
{ 0x23B, 1.0e0, "Reserved", "Reserved" },
{ 0x23C, 1.0e0, "Reserved", "Reserved" },
{ 0x23D, 1.0e0, "Reserved", "Reserved" },
{ 0x23E, 1.0e0, "Reserved", "Reserved" },
{ 0x23F, 1.0e0, "Reserved", "Reserved" },
{ 0x240, 1.0e0, "Reserved", "Reserved" },
{ 0x241, 1.0e0, "Reserved", "Reserved" },
{ 0x242, 1.0e0, "Reserved", "Reserved" },
{ 0x243, 1.0e0, "Reserved", "Reserved" },
{ 0x244, 1.0e0, "Reserved", "Reserved" },
{ 0x245, 1.0e0, "Reserved", "Reserved" },
{ 0x246, 1.0e0, "Reserved", "Reserved" },
{ 0x247, 1.0e0, "Reserved", "Reserved" },
{ 0x248, 1.0e0, "Reserved", "Reserved" },
{ 0x249, 1.0e0, "Reserved", "Reserved" },
{ 0x24A, 1.0e0, "Reserved", "Reserved" },
{ 0x24B, 1.0e0, "Reserved", "Reserved" },
{ 0x24C, 1.0e0, "Reserved", "Reserved" },
{ 0x24D, 1.0e0, "Reserved", "Reserved" },
{ 0x24E, 1.0e0, "Reserved", "Reserved" },
{ 0x24F, 1.0e0, "Reserved", "Reserved" },
{ 0x250, 1.0e0, "Reserved", "Reserved" },
{ 0x251, 1.0e0, "Reserved", "Reserved" },
{ 0x252, 1.0e0, "Reserved", "Reserved" },
{ 0x253, 1.0e0, "Reserved", "Reserved" },
{ 0x254, 1.0e0, "Reserved", "Reserved" },
{ 0x255, 1.0e0, "Reserved", "Reserved" },
{ 0x256, 1.0e0, "Reserved", "Reserved" },
{ 0x257, 1.0e0, "Reserved", "Reserved" },

/* E101 10nn Flow Temperature 10(nn-3) °F 0.001°F to 1°F */
{ 0x258, 1.0e-3, "°F", "Flow temperature" },
{ 0x259, 1.0e-2, "°F", "Flow temperature" },
{ 0x25A, 1.0e-1, "°F", "Flow temperature" },
{ 0x25B, 1.0e0,  "°F", "Flow temperature" },

/* E101 11nn Return Temperature 10(nn-3) °F 0.001°F to 1°F */
{ 0x25C, 1.0e-3, "°F", "Return temperature" },
{ 0x25D, 1.0e-2, "°F", "Return temperature" },
{ 0x25E, 1.0e-1, "°F", "Return temperature" },
{ 0x25F, 1.0e0,  "°F", "Return temperature" },

/* E110 00nn Temperature Difference 10(nn-3) °F 0.001°F to 1°F */
{ 0x260, 1.0e-3, "°F", "Temperature difference" },
{ 0x261, 1.0e-2, "°F", "Temperature difference" },
{ 0x262, 1.0e-1, "°F", "Temperature difference" },
{ 0x263, 1.0e0,  "°F", "Temperature difference" },

/* E110 01nn External Temperature 10(nn-3) °F 0.001°F to 1°F */
{ 0x264, 1.0e-3, "°F", "External temperature" },
{ 0x265, 1.0e-2, "°F", "External temperature" },
{ 0x266, 1.0e-1, "°F", "External temperature" },
{ 0x267, 1.0e0,  "°F", "External temperature" },

/* E110 1nnn Reserved */
{ 0x268, 1.0e0, "Reserved", "Reserved" },
{ 0x269, 1.0e0, "Reserved", "Reserved" },
{ 0x26A, 1.0e0, "Reserved", "Reserved" },
{ 0x26B, 1.0e0, "Reserved", "Reserved" },
{ 0x26C, 1.0e0, "Reserved", "Reserved" },
{ 0x26D, 1.0e0, "Reserved", "Reserved" },
{ 0x26E, 1.0e0, "Reserved", "Reserved" },
{ 0x26F, 1.0e0, "Reserved", "Reserved" },

/* E111 00nn Cold / Warm Temperature Limit 10(nn-3) °F 0.001°F to 1°F */
{ 0x270, 1.0e-3, "°F", "Cold / Warm Temperature Limit" },
{ 0x271, 1.0e-2, "°F", "Cold / Warm Temperature Limit" },
{ 0x272, 1.0e-1, "°F", "Cold / Warm Temperature Limit" },
{ 0x273, 1.0e0,  "°F", "Cold / Warm Temperature Limit" },

/* E111 01nn Cold / Warm Temperature Limit 10(nn-3) °C 0.001°C to 1°C */
{ 0x274, 1.0e-3, "°C", "Cold / Warm Temperature Limit" },
{ 0x275, 1.0e-2, "°C", "Cold / Warm Temperature Limit" },
{ 0x276, 1.0e-1, "°C", "Cold / Warm Temperature Limit" },
{ 0x277, 1.0e0,  "°C", "Cold / Warm Temperature Limit" },

/* E111 1nnn cumul. count max power § 10(nnn-3) W 0.001W to 10000W */
{ 0x278, 1.0e-3, "W", "Cumul count max power" },
{ 0x279, 1.0e-3, "W", "Cumul count max power" },
{ 0x27A, 1.0e-1, "W", "Cumul count max power" },
{ 0x27B, 1.0e0,  "W", "Cumul count max power" },
{ 0x27C, 1.0e1,  "W", "Cumul count max power" },
{ 0x27D, 1.0e2,  "W", "Cumul count max power" },
{ 0x27E, 1.0e3,  "W", "Cumul count max power" },
{ 0x27F, 1.0e4,  "W", "Cumul count max power" },

/* End of array */
{ 0xFFFF, 0.0, "", "" },
};
mbus_variable_vif fixed_table[] = {
/* 00, 01 left out */
{ 0x02, 1.0e0, "Wh", "Energy" },
{ 0x03, 1.0e1, "Wh", "Energy" },
{ 0x04, 1.0e2, "Wh", "Energy" },
{ 0x05, 1.0e3, "Wh", "Energy" },
{ 0x06, 1.0e4, "Wh", "Energy" },
{ 0x07, 1.0e5, "Wh", "Energy" },
{ 0x08, 1.0e6, "Wh", "Energy" },
{ 0x09, 1.0e7, "Wh", "Energy" },
{ 0x0A, 1.0e8, "Wh", "Energy" },
{ 0x0B, 1.0e3, "J", "Energy" },
{ 0x0C, 1.0e4, "J", "Energy" },
{ 0x0D, 1.0e5, "J", "Energy" },
{ 0x0E, 1.0e6, "J", "Energy" },
{ 0x0F, 1.0e7, "J", "Energy" },
{ 0x10, 1.0e8, "J", "Energy" },
{ 0x11, 1.0e9, "J", "Energy" },
{ 0x12, 1.0e10,"J", "Energy" },
{ 0x13, 1.0e11,"J", "Energy" },

{ 0x14, 1.0e0, "W", "Power" },
{ 0x15, 1.0e0, "W", "Power" },
{ 0x16, 1.0e0, "W", "Power" },
{ 0x17, 1.0e0, "W", "Power" },
{ 0x18, 1.0e0, "W", "Power" },
{ 0x19, 1.0e0, "W", "Power" },
{ 0x1A, 1.0e0, "W", "Power" },
{ 0x1B, 1.0e0, "W", "Power" },
{ 0x1C, 1.0e0, "W", "Power" },

{ 0x1D, 1.0e3, "J/h", "Energy" },
{ 0x1E, 1.0e4, "J/h", "Energy" },
{ 0x1F, 1.0e5, "J/h", "Energy" },
{ 0x20, 1.0e6, "J/h", "Energy" },
{ 0x21, 1.0e7, "J/h", "Energy" },
{ 0x22, 1.0e8, "J/h", "Energy" },
{ 0x23, 1.0e9, "J/h", "Energy" },
{ 0x24, 1.0e10,"J/h", "Energy" },
{ 0x25, 1.0e11,"J/h", "Energy" },

{ 0x26, 1.0e-6,"m^3", "Volume" },
{ 0x27, 1.0e-5,"m^3", "Volume" },
{ 0x28, 1.0e-4,"m^3", "Volume" },
{ 0x29, 1.0e-3,"m^3", "Volume" },
{ 0x2A, 1.0e-2,"m^3", "Volume" },
{ 0x2B, 1.0e-1,"m^3", "Volume" },
{ 0x2C, 1.0e0, "m^3", "Volume" },
{ 0x2D, 1.0e1, "m^3", "Volume" },
{ 0x2E, 1.0e2, "m^3", "Volume" },

{ 0x2F, 1.0e-5,"m^3/h", "Volume flow" },
{ 0x31, 1.0e-4,"m^3/h", "Volume flow" },
{ 0x32, 1.0e-3,"m^3/h", "Volume flow" },
{ 0x33, 1.0e-2,"m^3/h", "Volume flow" },
{ 0x34, 1.0e-1,"m^3/h", "Volume flow" },
{ 0x35, 1.0e0, "m^3/h", "Volume flow" },
{ 0x36, 1.0e1, "m^3/h", "Volume flow" },
{ 0x37, 1.0e2, "m^3/h", "Volume flow" },

{ 0x38, 1.0e-3, "°C", "Temperature" },

{ 0x39, 1.0e0,  "Units for H.C.A.", "H.C.A." },

{ 0x3A, 0.0,  "Reserved", "Reserved" },
{ 0x3B, 0.0,  "Reserved", "Reserved" },
{ 0x3C, 0.0,  "Reserved", "Reserved" },
{ 0x3D, 0.0,  "Reserved", "Reserved" },

{ 0x3E, 1.0e0,  "", "historic" },

{ 0x3F, 1.0e0,  "", "No units" },

/* end of array */
{ 0xFFFF, 0.0, "", "" },
};
//------------------------------------------------------------------------------
/// Register a function for receive events.
//------------------------------------------------------------------------------
void
mbus_register_recv_event(mbus_handle * handle, void (*event)(unsigned char src_type, const char *buff, size_t len))
{
handle->recv_event = event;
}
//------------------------------------------------------------------------------
/// Register a function for send events.
//------------------------------------------------------------------------------
void
mbus_register_send_event(mbus_handle * handle, void (*event)(unsigned char src_type, const char *buff, size_t len))
{
handle->send_event = event;
}
//------------------------------------------------------------------------------
/// Register a function for the scan progress.
//------------------------------------------------------------------------------
void
mbus_register_scan_progress(mbus_handle * handle, void (*event)(mbus_handle * handle, const char *mask))
{
handle->scan_progress = event;
}
//------------------------------------------------------------------------------
/// Register a function for the found events.
//------------------------------------------------------------------------------
void
mbus_register_found_event(mbus_handle * handle, void (*event)(mbus_handle * handle, mbus_frame *frame))
{
handle->found_event = event;
}
int mbus_fixed_normalize(int medium_unit, long medium_value, char **unit_out, double *value_out, char **quantity_out)
{
medium_unit = medium_unit & 0x3F;
if (unit_out == NULL || value_out == NULL || quantity_out == NULL)
{
    MBUS_ERROR("%s: Invalid parameter.\n", __PRETTY_FUNCTION__);
    return -1;
}

switch (medium_unit)
{
    case 0x00:
        *unit_out = strdup("h,m,s"); /*  todo convert to unix time... */
        *quantity_out = strdup("Time");
        break;
    case 0x01:
        *unit_out = strdup("D,M,Y"); /*  todo convert to unix time... */
        *quantity_out = strdup("Time");
        break;

default:
    for(int i=0; fixed_table[i].vif < 0xfff; ++i)
    {
        if (fixed_table[i].vif == medium_unit)
        {
            *unit_out = strdup(fixed_table[i].unit);
            *value_out = ((double) (medium_value)) * fixed_table[i].exponent;
            *quantity_out = strdup(fixed_table[i].quantity);
            return 0;
        }
    }

    *unit_out = strdup("Unknown");
    *quantity_out = strdup("Unknown");
    *value_out = 0.0;
    return -1;
}

return -2;

}
int mbus_variable_value_decode(mbus_data_record *record, double *value_out_real, char **value_out_str, int *value_out_str_size)
{
int result = 0;
unsigned char vif, vife;
struct tm time;
int value_out_int;
long long value_out_long_long;
*value_out_real = 0.0;
*value_out_str = NULL;
*value_out_str_size = 0;
if (record)
{
    MBUS_DEBUG("coding = 0x%02X \n", record->drh.dib.dif);

    // ignore extension bit
    vif = (record->drh.vib.vif & MBUS_DIB_VIF_WITHOUT_EXTENSION);
    vife = (record->drh.vib.vife[0] & MBUS_DIB_VIF_WITHOUT_EXTENSION);

    switch (record->drh.dib.dif & MBUS_DATA_RECORD_DIF_MASK_DATA)
    {
        case 0x00: /* no data */
            if ((*value_out_str = (char*) malloc(1)) == NULL)
            {
                MBUS_ERROR("Unable to allocate memory");
                return -1;
            }
            *value_out_str[0] = '\0';
            *value_out_str_size = 0;
            result = 0;
            break;

        case 0x01: /* 1 byte integer (8 bit) */
            result = mbus_data_int_decode(record->data, 1, &value_out_int);
            *value_out_real = value_out_int;
            break;

        case 0x02: /* 2 byte integer (16 bit) */
            // E110 1100  Time Point (date)
            if (vif == 0x6C)
            {
                mbus_data_tm_decode(&time, record->data, 2);
                if ((*value_out_str = (char*) malloc(11)) == NULL)
                {
                    MBUS_ERROR("Unable to allocate memory");
                    return -1;
                }
                *value_out_str_size = snprintf(*value_out_str, 11, "%04d-%02d-%02d",
                                             (time.tm_year + 1900),
                                             (time.tm_mon + 1),
                                              time.tm_mday);
                result = 0;
            }
            else  // normal integer
            {
                result = mbus_data_int_decode(record->data, 2, &value_out_int);
                *value_out_real = value_out_int;
            }
            break;

        case 0x03: /* 3 byte integer (24 bit) */
            result = mbus_data_int_decode(record->data, 3, &value_out_int);
            *value_out_real = value_out_int;
            break;

        case 0x04: /* 4 byte integer (32 bit) */
            // E110 1101  Time Point (date/time)
            // E011 0000  Start (date/time) of tariff
            // E111 0000  Date and time of battery change
            if ( (vif == 0x6D) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x30)) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x70)))
            {
                mbus_data_tm_decode(&time, record->data, 4);
                if ((*value_out_str = (char*) malloc(21)) == NULL)
                {
                    MBUS_ERROR("Unable to allocate memory");
                    return -1;
                }
                *value_out_str_size = snprintf(*value_out_str, 21, "%04d-%02d-%02dT%02d:%02d:%02dZ",
                                             (time.tm_year + 1900),
                                             (time.tm_mon + 1),
                                              time.tm_mday,
                                              time.tm_hour,
                                              time.tm_min,
                                              time.tm_sec);
                result = 0;
            }
            else  // normal integer
            {
                result = mbus_data_int_decode(record->data, 4, &value_out_int);
                *value_out_real = value_out_int;
            }
            break;

        case 0x05: /* 32b real */
            *value_out_real = mbus_data_float_decode(record->data);
            result = 0;
            break;

        case 0x06: /* 6 byte integer (48 bit) */
            // E110 1101  Time Point (date/time)
            // E011 0000  Start (date/time) of tariff
            // E111 0000  Date and time of battery change
            if ( (vif == 0x6D) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x30)) ||
                ((record->drh.vib.vif == 0xFD) && (vife == 0x70)))
            {
                mbus_data_tm_decode(&time, record->data, 6);
                if ((*value_out_str = (char*) malloc(21)) == NULL)
                {
                    MBUS_ERROR("Unable to allocate memory");
                    return -1;
                }
                *value_out_str_size = snprintf(*value_out_str, 21, "%04d-%02d-%02dT%02d:%02d:%02dZ",
                                             (time.tm_year + 1900),
                                             (time.tm_mon + 1),
                                              time.tm_mday,
                                              time.tm_hour,
                                              time.tm_min,
                                              time.tm_sec);
                result = 0;
            }
            else  // normal integer
            {
                result = mbus_data_long_long_decode(record->data, 6, &value_out_long_long);
                *value_out_real = value_out_long_long;
            }
            break;

        case 0x07: /* 8 byte integer (64 bit) */
            result = mbus_data_long_long_decode(record->data, 8, &value_out_long_long);
            *value_out_real = value_out_long_long;
            break;

        case 0x09: /* 2 digit BCD (8 bit) */
            *value_out_real = mbus_data_bcd_decode(record->data, 1);
            result = 0;
            break;

        case 0x0A: /* 4 digit BCD (16 bit) */
            *value_out_real = mbus_data_bcd_decode(record->data, 2);
            result = 0;
            break;

        case 0x0B: /* 6 digit BCD (24 bit) */
            *value_out_real = mbus_data_bcd_decode(record->data, 3);
            result = 0;
            break;

        case 0x0C: /* 8 digit BCD (32 bit) */
            *value_out_real = mbus_data_bcd_decode(record->data, 4);
            result = 0;
            break;

        case 0x0D: /* variable length */
        {
            if (record->data_len <= 0xBF) {
                if ((*value_out_str = (char*) malloc(record->data_len + 1)) == NULL)
                {
                    MBUS_ERROR("Unable to allocate memory");
                    return -1;
                }
                *value_out_str_size = record->data_len;
                mbus_data_str_decode((unsigned char*)(*value_out_str), record->data, record->data_len);
                result = 0;
                break;
            }
            result = -2;
            MBUS_ERROR("Non ASCII variable length not implemented yet\n");
            break;
        }

        case 0x0E: /* 12 digit BCD (40 bit) */
            *value_out_real = mbus_data_bcd_decode(record->data, 6);
            result = 0;
            break;

        case 0x0F: /* Special functions */
            if ((*value_out_str = (char*) malloc(3 * record->data_len + 1)) == NULL)
            {
                MBUS_ERROR("Unable to allocate memory");
                return -1;
            }
            *value_out_str_size = 3 * record->data_len;
            mbus_data_bin_decode((unsigned char*)(*value_out_str), record->data, record->data_len, (3 * record->data_len + 1));
            result = 0;
            break;

        default:
            result = -2;
            MBUS_ERROR("Unknown DIF (0x%.2x)", record->drh.dib.dif);
            break;
    }
}
else
{
    MBUS_ERROR("record is null");
    result = -3;
}

return result;

}
int
mbus_vif_unit_normalize(int vif, double value, char **unit_out, double *value_out, char *quantity_out)
{
int i;
unsigned newVif = vif & 0xF7F; / clear extension bit */
MBUS_DEBUG("vif_unit_normalize = 0x%03X \n", vif);

if (unit_out == NULL || value_out == NULL || quantity_out == NULL)
{
    MBUS_ERROR("%s: Invalid parameter.\n", __PRETTY_FUNCTION__);
    return -1;
}

for(i=0; vif_table[i].vif < 0xfff; ++i)
{
    if (vif_table[i].vif == newVif)
    {
        *unit_out = strdup(vif_table[i].unit);
        *value_out = value * vif_table[i].exponent;
        *quantity_out = strdup(vif_table[i].quantity);
        return 0;
    }
}

MBUS_ERROR("%s: Unknown VIF 0x%03X\n", __PRETTY_FUNCTION__, newVif);
*unit_out = strdup("Unknown (VIF=0x%.02X)");
*quantity_out = strdup("Unknown");
*value_out = 0.0;
return -1;

}
int
mbus_vib_unit_normalize(mbus_value_information_block *vib, double value, char **unit_out, double *value_out, char **quantity_out)
{
int code;
if (vib == NULL || unit_out == NULL || value_out == NULL || quantity_out == NULL)
{
    MBUS_ERROR("%s: Invalid parameter.\n", __PRETTY_FUNCTION__);
    return -1;
}

MBUS_DEBUG("%s: vib_unit_normalize - VIF=0x%02X\n", __PRETTY_FUNCTION__, vib->vif);

if (vib->vif == 0xFD) /* first type of VIF extention: see table 8.4.4 a */
{
    if (vib->nvife == 0)
    {
        MBUS_ERROR("%s: Missing VIF extension\n", __PRETTY_FUNCTION__);
        return -1;
    }

    code = ((vib->vife[0]) & MBUS_DIB_VIF_WITHOUT_EXTENSION) | 0x100;
    if (mbus_vif_unit_normalize(code, value, unit_out, value_out, quantity_out) != 0)
    {
        MBUS_ERROR("%s: Error mbus_vif_unit_normalize\n", __PRETTY_FUNCTION__);
        return -1;
    }
} else {
    if (vib->vif == 0xFB) /* second type of VIF extention: see table 8.4.4 b */
    {
        if (vib->nvife == 0)
        {
            MBUS_ERROR("%s: Missing VIF extension\n", __PRETTY_FUNCTION__);
            return -1;
        }

        code = ((vib->vife[0]) & MBUS_DIB_VIF_WITHOUT_EXTENSION) | 0x200;
        if (0 != mbus_vif_unit_normalize(code, value, unit_out, value_out, quantity_out))
        {
            MBUS_ERROR("%s: Error mbus_vif_unit_normalize\n", __PRETTY_FUNCTION__);
            return -1;
        }
    }
    else if ((vib->vif == 0x7C) ||
             (vib->vif == 0xFC))
    {
        // custom VIF
        *unit_out = strdup("-");
        *quantity_out = strdup(vib->custom_vif);
        *value_out = value;
    }
    else
    {
        code = (vib->vif) & MBUS_DIB_VIF_WITHOUT_EXTENSION;
        if (0 != mbus_vif_unit_normalize(code, value, unit_out, value_out, quantity_out))
        {
            MBUS_ERROR("%s: Error mbus_vif_unit_normalize\n", __PRETTY_FUNCTION__);
            return -1;
        }
    }
}

if ((vib->vif & MBUS_DIB_VIF_EXTENSION_BIT) &&
    (vib->vif != 0xFD) &&
    (vib->vif != 0xFB))                       /* codes for VIF extention: see table 8.4.5 */
{
    code = (vib->vife[0]) & 0x7f;
    switch (code)
    {
        case 0x70:
        case 0x71:
        case 0x72:
        case 0x73:
        case 0x74:
        case 0x75:
        case 0x76:
        case 0x77: /* Multiplicative correction factor: 10^nnn-6 */
            *value_out *= pow(10.0, (vib->vife[0] & 0x07) - 6);
            break;

        case 0x78:
        case 0x79:
        case 0x7A:
        case 0x7B: /* Additive correction constant: 10^nn-3 unit of VIF (offset) */
            *value_out += pow(10.0, (vib->vife[0] & 0x03) - 3);
            break;

        case 0x7D: /* Multiplicative correction factor: 10^3 */
            *value_out *= 1000.0;
            break;
    }
}

return 0;

}
mbus_record *
mbus_record_new()
{
mbus_record * record;
if (!(record = (mbus_record *) malloc(sizeof(mbus_record))))
{
    MBUS_ERROR("%s: memory allocation error\n", __PRETTY_FUNCTION__);
    return NULL;
}

record->value.real_val = 0.0;
record->is_numeric = 1;
record->unit = NULL;
record->function_medium = NULL;
record->quantity = NULL;
record->device = -1;
record->tariff = -1;
record->storage_number = 0;
return record;

}
void
mbus_record_free(mbus_record * rec)
{
if (rec)
{
if (! rec->is_numeric)
{
free((rec->value).str_val.value);
(rec->value).str_val.value = NULL;
}
    if (rec->unit)
    {
        free(rec->unit);
        rec->unit = NULL;
    }

    if (rec->function_medium)
    {
        free(rec->function_medium);
        rec->function_medium = NULL;
    }

    if (rec->quantity)
    {
        free(rec->quantity);
        rec->quantity = NULL;
    }
    free(rec);
}

}
mbus_record *
mbus_parse_fixed_record(char status_byte, char medium_unit, unsigned char *data)
{
long value = 0;
mbus_record * record = NULL;
if (!(record = mbus_record_new()))
{
    MBUS_ERROR("%s: memory allocation error\n", __PRETTY_FUNCTION__);
    return NULL;
}

/* shared/static memory - get own copy */
record->function_medium = strdup(mbus_data_fixed_function((int)status_byte));  /* stored / actual */

if (record->function_medium == NULL)
{
    MBUS_ERROR("%s: memory allocation error\n", __PRETTY_FUNCTION__);
    mbus_record_free(record);
    return NULL;
}

if ((status_byte & MBUS_DATA_FIXED_STATUS_FORMAT_MASK) == MBUS_DATA_FIXED_STATUS_FORMAT_BCD)
{
    value = mbus_data_bcd_decode(data, 4);
}
else
{
    mbus_data_long_decode(data, 4, &value);
}

record->unit = NULL;
record->is_numeric = 1;
if (0 != mbus_fixed_normalize(medium_unit, value, &(record->unit), &(record->value.real_val), &(record->quantity)))
{
    MBUS_ERROR("Problem with mbus_fixed_normalize.\n");
    mbus_record_free(record);
    return NULL;
}

return record;

}
mbus_record *
mbus_parse_variable_record(mbus_data_record *data)
{
mbus_record * record = NULL;
double value_out_real = 0.0; /< raw value */
char * value_out_str = NULL;
int value_out_str_size = 0;
double real_val = 0.0; /< normalized value */
if (data == NULL)
{
    MBUS_ERROR("%s: Invalid record.\n", __PRETTY_FUNCTION__);
    return NULL;
}

if (!(record = mbus_record_new()))
{
    MBUS_ERROR("%s: memory allocation error\n", __PRETTY_FUNCTION__);
    return NULL;
}

record->storage_number = mbus_data_record_storage_number(data);
record->tariff = mbus_data_record_tariff(data);
record->device = mbus_data_record_device(data);

if ((data->drh.dib.dif == MBUS_DIB_DIF_MANUFACTURER_SPECIFIC) ||
    (data->drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW)) /* MBUS_DIB_DIF_VENDOR_SPECIFIC */
{
    if (data->drh.dib.dif == MBUS_DIB_DIF_MORE_RECORDS_FOLLOW)
    {
        record->function_medium = strdup("More records follow");
    }
    else
    {
        record->function_medium = strdup("Manufacturer specific");
    }

    if (record->function_medium == NULL)
    {
        MBUS_ERROR("%s: memory allocation error\n", __PRETTY_FUNCTION__);
        mbus_record_free(record);
        return NULL;
    }

    /* parsing of data not implemented yet
       manufacturer specific data structures to end of user data */

    if (mbus_variable_value_decode(data, &value_out_real, &value_out_str, &value_out_str_size) != 0)
    {
        MBUS_ERROR("%s: problem with mbus_variable_value_decode\n", __PRETTY_FUNCTION__);
        mbus_record_free(record);
        return NULL;
    }

    if (value_out_str != NULL)
    {
        record->is_numeric = 0;
        (record->value).str_val.value = value_out_str;
        (record->value).str_val.size = value_out_str_size;
    }
    else
    {
        record->is_numeric = 1;
        (record->value).real_val = real_val;
    }
}
else
{
    record->function_medium = strdup(mbus_data_record_function(data));

    if (record->function_medium == NULL)
    {
        MBUS_ERROR("%s: memory allocation error\n", __PRETTY_FUNCTION__);
        mbus_record_free(record);
        return NULL;
    }

    MBUS_DEBUG("record->function_medium = %s \n", record->function_medium);

    if (mbus_variable_value_decode(data, &value_out_real, &value_out_str, &value_out_str_size) != 0)
    {
        MBUS_ERROR("%s: problem with mbus_variable_value_decode\n", __PRETTY_FUNCTION__);
        mbus_record_free(record);
        return NULL;
    }
    MBUS_DEBUG("value_out_real = %lf \n", value_out_real);

    if (mbus_vib_unit_normalize(&(data->drh.vib), value_out_real, &(record->unit), &real_val, &(record->quantity)) != 0)
    {
        MBUS_ERROR("%s: problem with mbus_vib_unit_normalize\n", __PRETTY_FUNCTION__);
        mbus_record_free(record);
        record = NULL;
        return NULL;
    }
    MBUS_DEBUG("record->unit = %s \n", record->unit);
    MBUS_DEBUG("real_val = %lf \n", real_val);

    if (value_out_str != NULL)
    {
        record->is_numeric = 0;
        (record->value).str_val.value = value_out_str;
        (record->value).str_val.size = value_out_str_size;
    }
    else
    {
        record->is_numeric = 1;
        (record->value).real_val = real_val;
    }
}

return record;

}
//------------------------------------------------------------------------------
/// Generate XML for variable-length data
//------------------------------------------------------------------------------
char *
mbus_data_variable_xml_normalized(mbus_data_variable *data)
{
mbus_data_record *record;
mbus_record *norm_record;
char *buff = NULL, *new_buff = NULL;
char str_encoded[768] = "";
size_t len = 0, buff_size = 8192;
size_t i;
if (data)
{
    buff = (char*) malloc(buff_size);

    if (buff == NULL)
        return NULL;

    len += snprintf(&buff[len], buff_size - len, MBUS_XML_PROCESSING_INSTRUCTION);

    len += snprintf(&buff[len], buff_size - len, "<MBusData>\n\n");

    len += snprintf(&buff[len], buff_size - len, "%s", mbus_data_variable_header_xml(&(data->header)));

    for (record = data->record, i = 0; record; record = record->next, i++)
    {
        norm_record = mbus_parse_variable_record(record);

        if ((buff_size - len) < 1024)
        {
            buff_size *= 2;
            new_buff = (char*) realloc(buff,buff_size);

            if (new_buff == NULL)
            {
                mbus_record_free(norm_record);
                free(buff);
                return NULL;
            }

            buff = new_buff;
        }

        len += snprintf(&buff[len], buff_size - len, "    <DataRecord id=\"%zu\">\n", i);

        if (norm_record != NULL)
        {
            mbus_str_xml_encode(str_encoded, norm_record->function_medium, sizeof(str_encoded));
            len += snprintf(&buff[len], buff_size - len, "        <Function>%s</Function>\n", str_encoded);

            len += snprintf(&buff[len], buff_size - len, "        <StorageNumber>%ld</StorageNumber>\n", norm_record->storage_number);

            if (norm_record->tariff >= 0)
            {
                len += snprintf(&buff[len], buff_size - len, "        <Tariff>%ld</Tariff>\n", norm_record->tariff);
                len += snprintf(&buff[len], buff_size - len, "        <Device>%d</Device>\n", norm_record->device);
            }

            mbus_str_xml_encode(str_encoded, norm_record->unit, sizeof(str_encoded));

            len += snprintf(&buff[len], buff_size - len, "        <Unit>%s</Unit>\n", str_encoded);

            mbus_str_xml_encode(str_encoded, norm_record->quantity, sizeof(str_encoded));
            len += snprintf(&buff[len], buff_size - len, "        <Quantity>%s</Quantity>\n", str_encoded);


            if (norm_record->is_numeric)
            {
                len += snprintf(&buff[len], buff_size - len, "        <Value>%f</Value>\n", norm_record->value.real_val);
            }
            else
            {
                mbus_str_xml_encode(str_encoded, norm_record->value.str_val.value, sizeof(str_encoded));
                len += snprintf(&buff[len], buff_size - len, "        <Value>%s</Value>\n", str_encoded);
            }

            mbus_record_free(norm_record);
        }
        else
        {
        }

        len += snprintf(&buff[len], buff_size - len, "    </DataRecord>\n\n");
    }

    len += snprintf(&buff[len], buff_size - len, "</MBusData>\n");

    return buff;
}

return NULL;

}
//------------------------------------------------------------------------------
/// Return a string containing an XML representation of the M-BUS frame data.
//------------------------------------------------------------------------------
char *
mbus_frame_data_xml_normalized(mbus_frame_data *data)
{
if (data)
{
if (data->type == MBUS_DATA_TYPE_FIXED)
{
return mbus_data_fixed_xml(&(data->data_fix));
}
    if (data->type == MBUS_DATA_TYPE_VARIABLE)
    {
        return mbus_data_variable_xml_normalized(&(data->data_var));
    }
}

return NULL;

}
mbus_handle *
mbus_context_serial(const char *device)
{
mbus_handle *handle;
mbus_serial_data *serial_data;
char error_str[128];
if ((handle = (mbus_handle *) malloc(sizeof(mbus_handle))) == NULL)
{
    MBUS_ERROR("%s: Failed to allocate handle.\n", __PRETTY_FUNCTION__);
    return NULL;
}

if ((serial_data = (mbus_serial_data *)malloc(sizeof(mbus_serial_data))) == NULL)
{
    snprintf(error_str, sizeof(error_str), "%s: failed to allocate memory for handle\n", __PRETTY_FUNCTION__);
    mbus_error_str_set(error_str);
    free(handle);
    return NULL;
}

handle->max_data_retry = 3;
handle->max_search_retry = 1;
handle->is_serial = 1;
handle->purge_first_frame = MBUS_FRAME_PURGE_M2S;
handle->auxdata = serial_data;
handle->open = mbus_serial_connect;
handle->close = mbus_serial_disconnect;
handle->recv = mbus_serial_recv_frame;
handle->send = mbus_serial_send_frame;
handle->free_auxdata = mbus_serial_data_free;
handle->recv_event = NULL;
handle->send_event = NULL;
handle->scan_progress = NULL;
handle->found_event = NULL;

if ((serial_data->device = strdup(device)) == NULL)
{
    snprintf(error_str, sizeof(error_str), "%s: failed to allocate memory for device\n", __PRETTY_FUNCTION__);
    mbus_error_str_set(error_str);
    free(serial_data);
    free(handle);
    return NULL;
}

return handle;

}
mbus_handle *
mbus_context_tcp(const char *host, uint16_t port)
{
mbus_handle *handle;
mbus_tcp_data *tcp_data;
char error_str[128];
if ((handle = (mbus_handle *) malloc(sizeof(mbus_handle))) == NULL)
{
    MBUS_ERROR("%s: Failed to allocate handle.\n", __PRETTY_FUNCTION__);
    return NULL;
}

if ((tcp_data = (mbus_tcp_data *)malloc(sizeof(mbus_tcp_data))) == NULL)
{
    snprintf(error_str, sizeof(error_str), "%s: failed to allocate memory for handle\n", __PRETTY_FUNCTION__);
    mbus_error_str_set(error_str);
    free(handle);
    return NULL;
}

handle->max_data_retry = 3;
handle->max_search_retry = 1;
handle->is_serial = 0;
handle->purge_first_frame = MBUS_FRAME_PURGE_M2S;
handle->auxdata = tcp_data;
handle->open = mbus_tcp_connect;
handle->close = mbus_tcp_disconnect;
handle->recv = mbus_tcp_recv_frame;
handle->send = mbus_tcp_send_frame;
handle->free_auxdata = mbus_tcp_data_free;
handle->recv_event = NULL;
handle->send_event = NULL;
handle->scan_progress = NULL;
handle->found_event = NULL;

tcp_data->port = port;
if ((tcp_data->host = strdup(host)) == NULL)
{
    snprintf(error_str, sizeof(error_str), "%s: failed to allocate memory for host\n", __PRETTY_FUNCTION__);
    mbus_error_str_set(error_str);
    free(tcp_data);
    free(handle);
    return NULL;
}

return handle;

}
void
mbus_context_free(mbus_handle * handle)
{
if (handle)
{
handle->free_auxdata(handle);
free(handle);
}
}
int
mbus_connect(mbus_handle * handle)
{
if (handle == NULL)
{
MBUS_ERROR("%s: Invalid M-Bus handle for disconnect.\n", PRETTY_FUNCTION);
return -1;
}
return handle->open(handle);

}
int
mbus_disconnect(mbus_handle * handle)
{
if (handle == NULL)
{
MBUS_ERROR("%s: Invalid M-Bus handle for disconnect.\n", PRETTY_FUNCTION);
return -1;
}
return handle->close(handle);

}
int
mbus_context_set_option(mbus_handle * handle, mbus_context_option option, long value)
{
if (handle == NULL)
{
MBUS_ERROR("%s: Invalid M-Bus handle to set option.\n", PRETTY_FUNCTION);
return -1;
}
switch (option)
{
    case MBUS_OPTION_MAX_DATA_RETRY:
        if ((value >= 0) && (value <= 9))
        {
            handle->max_data_retry = value;
            return 0;
        }
        break;
    case MBUS_OPTION_MAX_SEARCH_RETRY:
        if ((value >= 0) && (value <= 9))
        {
            handle->max_search_retry = value;
            return 0;
        }
        break;
    case MBUS_OPTION_PURGE_FIRST_FRAME:
        if ((value == MBUS_FRAME_PURGE_NONE) ||
            (value == MBUS_FRAME_PURGE_M2S)  ||
            (value == MBUS_FRAME_PURGE_S2M))
        {
            handle->purge_first_frame = value;
            return 0;
        }
        break;
}

return -1; // unable to set option

}
int
mbus_recv_frame(mbus_handle * handle, mbus_frame *frame)
{
int result = 0;
if (handle == NULL)
{
    MBUS_ERROR("%s: Invalid M-Bus handle for receive.\n", __PRETTY_FUNCTION__);
    return MBUS_RECV_RESULT_ERROR;
}

if (frame == NULL)
{
    MBUS_ERROR("%s: Invalid frame.\n", __PRETTY_FUNCTION__);
    return MBUS_RECV_RESULT_ERROR;
}

result = handle->recv(handle, frame);

switch (mbus_frame_direction(frame))
{
    case MBUS_CONTROL_MASK_DIR_M2S:
        if (handle->purge_first_frame == MBUS_FRAME_PURGE_M2S)
            result = handle->recv(handle, frame);  // purge echo and retry
        break;
    case MBUS_CONTROL_MASK_DIR_S2M:
        if (handle->purge_first_frame == MBUS_FRAME_PURGE_S2M)
            result = handle->recv(handle, frame);  // purge echo and retry
        break;
}

if (frame != NULL)
{
    /* set timestamp to receive time */
    time(&(frame->timestamp));
}

return result;

}
int mbus_purge_frames(mbus_handle *handle)
{
int err, received;
mbus_frame reply;
memset((void *)&reply, 0, sizeof(mbus_frame));

received = 0;
while (1)
{
    err = mbus_recv_frame(handle, &reply);
    if (err != MBUS_RECV_RESULT_OK &&
        err != MBUS_RECV_RESULT_INVALID)
        break;

    received = 1;
}

return received;

}
int
mbus_send_frame(mbus_handle * handle, mbus_frame *frame)
{
if (handle == NULL)
{
MBUS_ERROR("%s: Invalid M-Bus handle for send.\n", PRETTY_FUNCTION);
return 0;
}
return handle->send(handle, frame);

}
//------------------------------------------------------------------------------
// send a data request packet to from master to slave: the packet selects
// a slave to be the active secondary addressed slave if the secondary address
// matches that of the slave.
//------------------------------------------------------------------------------
int
mbus_send_select_frame(mbus_handle * handle, const char *secondary_addr_str)
{
mbus_frame *frame;
frame = mbus_frame_new(MBUS_FRAME_TYPE_LONG);

if (mbus_frame_select_secondary_pack(frame, (char*) secondary_addr_str) == -1)
{
    MBUS_ERROR("%s: Failed to pack selection mbus frame.\n", __PRETTY_FUNCTION__);
    mbus_frame_free(frame);
    return -1;
}

if (mbus_send_frame(handle, frame) == -1)
{
    MBUS_ERROR("%s: Failed to send mbus frame.\n", __PRETTY_FUNCTION__);
    mbus_frame_free(frame);
    return -1;
}

mbus_frame_free(frame);
return 0;

}
//------------------------------------------------------------------------------
// send a user data packet from master to slave: the packet let the
// adressed slave(s) switch to the given baudrate
//------------------------------------------------------------------------------
int
mbus_send_switch_baudrate_frame(mbus_handle * handle, int address, long baudrate)
{
int retval = 0;
int control_information = 0;
mbus_frame *frame;
if (mbus_is_primary_address(address) == 0)
{
    MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, address);
    return -1;
}

switch (baudrate)
{
  case 300:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_300;
    break;
  case 600:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_600;
    break;
  case 1200:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_1200;
    break;
  case 2400:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_2400;
    break;
  case 4800:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_4800;
    break;
  case 9600:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_9600;
    break;
  case 19200:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_19200;
    break;
  case 38400:
    control_information = MBUS_CONTROL_INFO_SET_BAUDRATE_38400;
    break;
  default:
    MBUS_ERROR("%s: invalid baudrate %ld\n", __PRETTY_FUNCTION__, baudrate);
    return -1;
}

frame = mbus_frame_new(MBUS_FRAME_TYPE_CONTROL);

if (frame == NULL)
{
    MBUS_ERROR("%s: failed to allocate mbus frame.\n", __PRETTY_FUNCTION__);
    return -1;
}

frame->control = MBUS_CONTROL_MASK_SND_UD | MBUS_CONTROL_MASK_DIR_M2S;
frame->address = address;
frame->control_information = control_information;

if (mbus_send_frame(handle, frame) == -1)
{
    MBUS_ERROR("%s: failed to send mbus frame.\n", __PRETTY_FUNCTION__);
    retval = -1;
}

mbus_frame_free(frame);
return retval;

}
//------------------------------------------------------------------------------
// send a user data packet from master to slave: the packet resets
// the application layer in the slave
//------------------------------------------------------------------------------
int
mbus_send_application_reset_frame(mbus_handle * handle, int address, int subcode)
{
int retval = 0;
mbus_frame *frame;
if (mbus_is_primary_address(address) == 0)
{
    MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, address);
    return -1;
}

if (subcode > 0xFF)
{
    MBUS_ERROR("%s: invalid subcode %d\n", __PRETTY_FUNCTION__, subcode);
    return -1;
}

frame = mbus_frame_new(MBUS_FRAME_TYPE_LONG);

if (frame == NULL)
{
    MBUS_ERROR("%s: failed to allocate mbus frame.\n", __PRETTY_FUNCTION__);
    return -1;
}

frame->control = MBUS_CONTROL_MASK_SND_UD | MBUS_CONTROL_MASK_DIR_M2S;
frame->address = address;
frame->control_information = MBUS_CONTROL_INFO_APPLICATION_RESET;

if (subcode >= 0)
{
    frame->data_size = 1;
    frame->data[0] = (subcode & 0xFF);
}
else
{
    frame->data_size = 0;
}

if (mbus_send_frame(handle, frame) == -1)
{
    MBUS_ERROR("%s: failed to send mbus frame.\n", __PRETTY_FUNCTION__);
    retval = -1;
}

mbus_frame_free(frame);
return retval;

}
//------------------------------------------------------------------------------
// send a request packet to from master to slave
//------------------------------------------------------------------------------
int
mbus_send_request_frame(mbus_handle * handle, int address)
{
int retval = 0;
mbus_frame *frame;
if (mbus_is_primary_address(address) == 0)
{
    MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, address);
    return -1;
}

frame = mbus_frame_new(MBUS_FRAME_TYPE_SHORT);

if (frame == NULL)
{
    MBUS_ERROR("%s: failed to allocate mbus frame.\n", __PRETTY_FUNCTION__);
    return -1;
}

frame->control = MBUS_CONTROL_MASK_REQ_UD2 | MBUS_CONTROL_MASK_DIR_M2S;
frame->address = address;

if (mbus_send_frame(handle, frame) == -1)
{
    MBUS_ERROR("%s: failed to send mbus frame.\n", __PRETTY_FUNCTION__);
    retval = -1;
}

mbus_frame_free(frame);
return retval;

}
//------------------------------------------------------------------------------
// send a user data packet from master to slave
//------------------------------------------------------------------------------
int
mbus_send_user_data_frame(mbus_handle * handle, int address, const unsigned char *data, size_t data_size)
{
int retval = 0;
mbus_frame *frame;
if (mbus_is_primary_address(address) == 0)
{
    MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, address);
    return -1;
}

if (data == NULL)
{
    MBUS_ERROR("%s: Invalid data\n", __PRETTY_FUNCTION__);
    return -1;
}

if ((data_size > MBUS_FRAME_DATA_LENGTH) || (data_size == 0))
{
    MBUS_ERROR("%s: illegal data_size %zu\n", __PRETTY_FUNCTION__, data_size);
    return -1;
}

frame = mbus_frame_new(MBUS_FRAME_TYPE_LONG);

if (frame == NULL)
{
    MBUS_ERROR("%s: failed to allocate mbus frame.\n", __PRETTY_FUNCTION__);
    return -1;
}

frame->control = MBUS_CONTROL_MASK_SND_UD | MBUS_CONTROL_MASK_DIR_M2S;
frame->address = address;
frame->control_information = MBUS_CONTROL_INFO_DATA_SEND;
frame->data_size = data_size;
memcpy(frame->data, data, data_size);

if (mbus_send_frame(handle, frame) == -1)
{
    MBUS_ERROR("%s: failed to send mbus frame.\n", __PRETTY_FUNCTION__);
    retval = -1;
}

mbus_frame_free(frame);
return retval;

}
//------------------------------------------------------------------------------
// send a request from master to slave in order to change the primary address
//------------------------------------------------------------------------------
int
mbus_set_primary_address(mbus_handle * handle, int old_address, int new_address)
{
/* primary address record, see chapter 6.4.2 */
unsigned char buffer
3
 = { 0x01, 0x7A, new_address };
if (mbus_is_primary_address(new_address) == 0)
{
    MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, new_address);
    return -1;
}

switch (new_address)
{
    case MBUS_ADDRESS_NETWORK_LAYER:
    case MBUS_ADDRESS_BROADCAST_REPLY:
    case MBUS_ADDRESS_BROADCAST_NOREPLY:
        MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, new_address);
        return -1;
}

return mbus_send_user_data_frame(handle, old_address, buffer, sizeof(buffer));

}
//------------------------------------------------------------------------------
// send a request from master to slave and collect the reply (replies)
// from the slave.
//------------------------------------------------------------------------------
int
mbus_sendrecv_request(mbus_handle *handle, int address, mbus_frame *reply, int max_frames)
{
int retval = 0, more_frames = 1, retry = 0;
mbus_frame_data reply_data;
mbus_frame *frame, *next_frame;
int frame_count = 0, result;
if (handle == NULL)
{
    MBUS_ERROR("%s: Invalid M-Bus handle for request.\n", __PRETTY_FUNCTION__);
    return 1;
}

if (mbus_is_primary_address(address) == 0)
{
    MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, address);
    return 1;
}

frame = mbus_frame_new(MBUS_FRAME_TYPE_SHORT);

if (frame == NULL)
{
    MBUS_ERROR("%s: failed to allocate mbus frame.\n", __PRETTY_FUNCTION__);
    return -1;
}

frame->control = MBUS_CONTROL_MASK_REQ_UD2 |
                 MBUS_CONTROL_MASK_DIR_M2S |
                 MBUS_CONTROL_MASK_FCV     |
                 MBUS_CONTROL_MASK_FCB;

frame->address = address;

//
// continue to read until no more records are available (usually only one
// reply frame, but can be more for so-called multi-telegram replies)
//
next_frame = reply;

memset((void *)&reply_data, 0, sizeof(mbus_frame_data));

while (more_frames)
{
    if (retry > handle->max_data_retry)
    {
        // Give up
        retval = 1;
        break;
    }

    if (debug)
        printf("%s: debug: sending request frame\n", __PRETTY_FUNCTION__);

    if (mbus_send_frame(handle, frame) == -1)
    {
        MBUS_ERROR("%s: failed to send mbus frame.\n", __PRETTY_FUNCTION__);
        retval = -1;
        break;
    }

    if (debug)
        printf("%s: debug: receiving response frame #%d\n", __PRETTY_FUNCTION__, frame_count);

    result = mbus_recv_frame(handle, next_frame);

    if (result == MBUS_RECV_RESULT_OK)
    {
        retry = 0;
        mbus_purge_frames(handle);
    }
    else if (result == MBUS_RECV_RESULT_TIMEOUT)
    {
        MBUS_ERROR("%s: No M-Bus response frame received.\n", __PRETTY_FUNCTION__);
        retry++;
        continue;
    }
    else if (result == MBUS_RECV_RESULT_INVALID)
    {
        MBUS_ERROR("%s: Received invalid M-Bus response frame.\n", __PRETTY_FUNCTION__);
        retry++;
        mbus_purge_frames(handle);
        continue;
    }
    else
    {
        MBUS_ERROR("%s: Failed to receive M-Bus response frame.\n", __PRETTY_FUNCTION__);
        retval = 1;
        break;
    }

    frame_count++;

    //
    // We need to parse the data in the received frame to be able to tell
    // if more records are available or not.
    //
    if (mbus_frame_data_parse(next_frame, &reply_data) == -1)
    {
        MBUS_ERROR("%s: M-bus data parse error.\n", __PRETTY_FUNCTION__);
        retval = 1;
        break;
    }

    //
    // Continue a cycle of sending requests and reading replies until the
    // reply do not have DIF=0x1F in the last record (which signals that
    // more records are available.
    //

    if (reply_data.type != MBUS_DATA_TYPE_VARIABLE)
    {
        // only single frame replies for FIXED type frames
        more_frames = 0;
    }
    else
    {
        more_frames = 0;

        if (reply_data.data_var.more_records_follow &&
            ((max_frames > 0) && (frame_count < max_frames))) // only readout max_frames
        {
            if (debug)
                printf("%s: debug: expecting more frames\n", __PRETTY_FUNCTION__);

            more_frames = 1;

            // allocate new frame and increment next_frame pointer
            next_frame->next = mbus_frame_new(MBUS_FRAME_TYPE_ANY);

            if (next_frame->next == NULL)
            {
                MBUS_ERROR("%s: failed to allocate mbus frame.\n", __PRETTY_FUNCTION__);
                retval = -1;
                more_frames = 0;
            }

            next_frame = next_frame->next;

            // toogle FCB bit
            frame->control ^= MBUS_CONTROL_MASK_FCB;
        }
        else
        {
            if (debug)
                printf("%s: debug: no more frames\n", __PRETTY_FUNCTION__);
        }
    }

    if (reply_data.data_var.record)
    {
        // free's up the whole list
        mbus_data_record_free(reply_data.data_var.record);
    }
}

mbus_frame_free(frame);
return retval;

}
//------------------------------------------------------------------------------
// send a data request packet to from master to slave and optional purge response
//------------------------------------------------------------------------------
int
mbus_send_ping_frame(mbus_handle *handle, int address, char purge_response)
{
int retval = 0;
mbus_frame *frame;
if (mbus_is_primary_address(address) == 0)
{
    MBUS_ERROR("%s: invalid address %d\n", __PRETTY_FUNCTION__, address);
    return 1;
}

frame = mbus_frame_new(MBUS_FRAME_TYPE_SHORT);

if (frame == NULL)
{
    MBUS_ERROR("%s: failed to allocate mbus frame.\n", __PRETTY_FUNCTION__);
    return -1;
}

frame->control  = MBUS_CONTROL_MASK_SND_NKE | MBUS_CONTROL_MASK_DIR_M2S;
frame->address  = address;

if (mbus_send_frame(handle, frame) == -1)
{
    MBUS_ERROR("%s: failed to send mbus frame.\n", __PRETTY_FUNCTION__);
    mbus_frame_free(frame);
    return -1;
}

if (purge_response)
{
    mbus_purge_frames(handle);
}

mbus_frame_free(frame);
return retval;

}
//------------------------------------------------------------------------------
// Select a device using the supplied secondary address (mask).
//------------------------------------------------------------------------------
int
mbus_select_secondary_address(mbus_handle * handle, const char *mask)
{
int ret;
mbus_frame reply;
if (mask == NULL || strlen(mask) != 16)
{
    MBUS_ERROR("%s: Invalid address masks.\n", __PRETTY_FUNCTION__);
    return MBUS_PROBE_ERROR;
}

/* send select command */
if (mbus_send_select_frame(handle, mask) == -1)
{
    MBUS_ERROR("%s: Failed to send selection frame: %s.\n",
               __PRETTY_FUNCTION__,
               mbus_error_str());
    return MBUS_PROBE_ERROR;
}

memset((void *)&reply, 0, sizeof(mbus_frame));
ret = mbus_recv_frame(handle, &reply);

if (ret == MBUS_RECV_RESULT_TIMEOUT)
{
    return MBUS_PROBE_NOTHING;
}

if (ret == MBUS_RECV_RESULT_INVALID)
{
    /* check for more data (collision) */
    mbus_purge_frames(handle);
    return MBUS_PROBE_COLLISION;
}

if (mbus_frame_type(&reply) == MBUS_FRAME_TYPE_ACK)
{
    /* check for more data (collision) */
    if (mbus_purge_frames(handle))
    {
        return MBUS_PROBE_COLLISION;
    }

    return MBUS_PROBE_SINGLE;
}

MBUS_ERROR("%s: Unexpected reply for address [%s].\n", __PRETTY_FUNCTION__, mask);

return MBUS_PROBE_NOTHING;

}
//------------------------------------------------------------------------------
// Probe for the presence of a device(s) using the supplied secondary address
// (mask).
//------------------------------------------------------------------------------
int
mbus_probe_secondary_address(mbus_handle *handle, const char *mask, char *matching_addr)
{
int ret, i;
mbus_frame reply;
if (mask == NULL || matching_addr == NULL || strlen(mask) != 16)
{
    MBUS_ERROR("%s: Invalid address masks.\n", __PRETTY_FUNCTION__);
    return MBUS_PROBE_ERROR;
}

for (i = 0; i <= handle->max_search_retry; i++)
{
    ret = mbus_select_secondary_address(handle, mask);

    if (ret == MBUS_PROBE_SINGLE)
    {
        /* send a data request command to find out the full address */
        if (mbus_send_request_frame(handle, MBUS_ADDRESS_NETWORK_LAYER) == -1)
        {
            MBUS_ERROR("%s: Failed to send request to selected secondary device [mask %s]: %s.\n",
                       __PRETTY_FUNCTION__,
                       mask,
                       mbus_error_str());
            return MBUS_PROBE_ERROR;
        }

        memset((void *)&reply, 0, sizeof(mbus_frame));
        ret = mbus_recv_frame(handle, &reply);

        if (ret == MBUS_RECV_RESULT_TIMEOUT)
        {
            return MBUS_PROBE_NOTHING;
        }

        if (ret == MBUS_RECV_RESULT_INVALID)
        {
            /* check for more data (collision) */
            mbus_purge_frames(handle);
            return MBUS_PROBE_COLLISION;
        }

        /* check for more data (collision) */
        if (mbus_purge_frames(handle))
        {
            return MBUS_PROBE_COLLISION;
        }

        if (mbus_frame_type(&reply) == MBUS_FRAME_TYPE_LONG)
        {
            char *addr = mbus_frame_get_secondary_address(&reply);

            if (addr == NULL)
            {
                // show error message, but procede with scan
                MBUS_ERROR("Failed to generate secondary address from M-Bus reply frame: %s\n",
                           mbus_error_str());
                return MBUS_PROBE_NOTHING;
            }

            snprintf(matching_addr, 17, "%s", addr);

            if (handle->found_event)
            {
                handle->found_event(handle,&reply);
            }

            return MBUS_PROBE_SINGLE;
        }
        else
        {
            MBUS_ERROR("%s: Unexpected reply for address [mask %s]. Expected long frame.\n",
                       __PRETTY_FUNCTION__, mask);
            return MBUS_PROBE_NOTHING;
        }
    }
    else if ((ret == MBUS_PROBE_ERROR) ||
             (ret == MBUS_PROBE_COLLISION))
    {
        break;
    }
}

return ret;

}
int mbus_read_slave(mbus_handle * handle, mbus_address *address, mbus_frame * reply)
{
if (handle == NULL || address == NULL)
{
MBUS_ERROR("%s: Invalid handle or address.\n", PRETTY_FUNCTION);
return -1;
}
if (address->is_primary)
{
    if (mbus_send_request_frame(handle, address->primary) == -1)
    {
        MBUS_ERROR("%s: Failed to send M-Bus request frame.\n",
                   __PRETTY_FUNCTION__);
        return -1;
    }
}
else
{
    /* secondary addressing */
    int probe_ret;

    if (address->secondary == NULL)
    {
        MBUS_ERROR("%s: Secondary address not set.\n",
                   __PRETTY_FUNCTION__);
        return -1;
    }

    probe_ret = mbus_select_secondary_address(handle, address->secondary);

    if (probe_ret == MBUS_PROBE_COLLISION)
    {
        MBUS_ERROR("%s: The address mask [%s] matches more than one device.\n",
                   __PRETTY_FUNCTION__,
                   address->secondary);
        return -1;
    }
    else if (probe_ret == MBUS_PROBE_NOTHING)
    {
        MBUS_ERROR("%s: The selected secondary address [%s] does not match any device.\n",
                   __PRETTY_FUNCTION__,
                   address->secondary);
        return -1;
    }
    else if (probe_ret == MBUS_PROBE_ERROR)
    {
        MBUS_ERROR("%s: Failed to probe secondary address [%s].\n",
                   __PRETTY_FUNCTION__,
                   address->secondary);
        return -1;
    }
    /* else MBUS_PROBE_SINGLE */

    if (mbus_send_request_frame(handle, MBUS_ADDRESS_NETWORK_LAYER) == -1)
    {
        MBUS_ERROR("%s: Failed to send M-Bus request frame.\n",
                   __PRETTY_FUNCTION__);
        return -1;
    }
}

if (mbus_recv_frame(handle, reply) != 0)
{
    MBUS_ERROR("%s: Failed to receive M-Bus response frame.\n",
               __PRETTY_FUNCTION__);
    return -1;
}

return 0;

}
//------------------------------------------------------------------------------
// Iterate over all address masks according to the M-Bus probe algorithm.
//------------------------------------------------------------------------------
int
mbus_scan_2nd_address_range(mbus_handle * handle, int pos, char *addr_mask)
{
int i, i_start, i_end, probe_ret;
char *mask, matching_mask
17
;
if (handle == NULL || addr_mask == NULL)
{
    MBUS_ERROR("%s: Invalid handle or address mask.\n", __PRETTY_FUNCTION__);
    return -1;
}

if (strlen(addr_mask) != 16)
{
    MBUS_ERROR("%s: Illegal address mask [%s]. Not 16 characters long.\n", __PRETTY_FUNCTION__, addr_mask);
    return -1;
}

if (pos < 0 || pos >= 16)
{
    return 0;
}

if ((mask = strdup(addr_mask)) == NULL)
{
    MBUS_ERROR("%s: Failed to allocate local copy of the address mask.\n", __PRETTY_FUNCTION__);
    return -1;
}

if (mask[pos] == 'f' || mask[pos] == 'F')
{
    // mask[pos] is a wildcard -> enumerate all 0..9 at this position
    i_start = 0;
    i_end   = 9;
}
else
{
    if (pos < 15)
    {
        // mask[pos] is not a wildcard -> don't iterate, recursively check pos+1
        mbus_scan_2nd_address_range(handle, pos+1, mask);
    }
    else
    {
        // .. except if we're at the last pos (==15) and this isn't a wildcard we still need to send the probe
        i_start = (int)(mask[pos] - '0');
        i_end   = (int)(mask[pos] - '0');
    }
}

// skip the scanning if we're returning from the (pos < 15) case above
if (mask[pos] == 'f' || mask[pos] == 'F' || pos == 15)
{
    for (i = i_start; i <= i_end; i++)
    {
        mask[pos] = '0'+i;

        if (handle->scan_progress)
            handle->scan_progress(handle,mask);

        probe_ret = mbus_probe_secondary_address(handle, mask, matching_mask);

        if (probe_ret == MBUS_PROBE_SINGLE)
        {
            if (!handle->found_event)
            {
                printf("Found a device on secondary address %s [using address mask %s]\n", matching_mask, mask);
            }
        }
        else if (probe_ret == MBUS_PROBE_COLLISION)
        {
            // collision, more than one device matching, restrict the search mask further
            mbus_scan_2nd_address_range(handle, pos+1, mask);
        }
        else if (probe_ret == MBUS_PROBE_NOTHING)
        {
             // nothing... move on to next address mask
        }
        else // MBUS_PROBE_ERROR
        {
            MBUS_ERROR("%s: Failed to probe secondary address [%s].\n", __PRETTY_FUNCTION__, mask);
            return -1;
        }
    }
}

free(mask);
return 0;

}
//------------------------------------------------------------------------------
// Convert a buffer with hex values into a buffer with binary values.
// - invalid character stops convertion
// - whitespaces will be ignored
//------------------------------------------------------------------------------
size_t
mbus_hex2bin(unsigned char * dst, size_t dst_len, const unsigned char * src, size_t src_len)
{
size_t i, result = 0;
unsigned long val;
unsigned char *ptr, *end, buf
3
;
if (!src || !dst)
{
    return 0;
}

memset(buf, 0, sizeof(buf));
memset(dst, 0, dst_len);

for (i = 0; i+1 < src_len; i++)
{
    // ignore whitespace
    if (isspace(src[i]))
        continue;

    buf[0] = src[i];
    buf[1] = src[++i];

    end = buf;
    ptr = end;
    val = strtoul(ptr, (char **)&end, 16);

    // abort at non hex value
    if (ptr == end)
        break;

    // abort at end of buffer
    if (result >= dst_len)
        break;

    dst[result++] = (unsigned char) val;
}

return result;

}
